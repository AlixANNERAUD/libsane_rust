//! Scan parameters and scanned frames.

use sane_sys as sys;

use crate::error::{Error, Result};

/// The format of a frame (`SANE_Frame`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FrameFormat {
    /// One gray (luminance) sample per pixel.
    Gray,
    /// Red, green and blue samples, interleaved per pixel.
    Rgb,
    /// The red samples of a three-pass color scan.
    Red,
    /// The green samples of a three-pass color scan.
    Green,
    /// The blue samples of a three-pass color scan.
    Blue,
    /// A format outside the SANE 1 standard (some backends send infrared or
    /// compressed frames).
    Unknown(u32),
}

impl FrameFormat {
    pub(crate) fn from_raw(raw: sys::SANE_Frame) -> Self {
        match raw {
            sys::SANE_FRAME_GRAY => Self::Gray,
            sys::SANE_FRAME_RGB => Self::Rgb,
            sys::SANE_FRAME_RED => Self::Red,
            sys::SANE_FRAME_GREEN => Self::Green,
            sys::SANE_FRAME_BLUE => Self::Blue,
            other => Self::Unknown(other),
        }
    }

    /// The number of samples per pixel (3 for RGB, 1 otherwise).
    pub fn channels(self) -> usize {
        match self {
            Self::Rgb => 3,
            _ => 1,
        }
    }
}

/// The parameters of a scan (`SANE_Parameters`).
///
/// Before [`Device::start`](crate::Device::start) they are an estimate; after
/// it, they describe the frame being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Parameters {
    /// The format of the frame.
    pub format: FrameFormat,
    /// Whether this frame is the last one of the image (three-pass scanners
    /// send three frames).
    pub last_frame: bool,
    /// The number of bytes per line, padding included.
    pub bytes_per_line: usize,
    /// The number of pixels per line.
    pub pixels_per_line: usize,
    /// The number of lines, `None` when unknown in advance (hand scanners,
    /// document feeders that detect the page end).
    pub lines: Option<usize>,
    /// The number of bits per sample: 1, 8 or 16. 16-bit samples are in the
    /// byte order of the host.
    pub depth: u32,
}

impl Parameters {
    /// Checks and converts the parameters returned by a backend.
    pub(crate) fn from_raw(raw: &sys::SANE_Parameters) -> Result<Self> {
        let size = |value: sys::SANE_Int, name: &'static str| {
            usize::try_from(value).map_err(|_| Error::Protocol(name))
        };
        Ok(Self {
            format: FrameFormat::from_raw(raw.format),
            last_frame: raw.last_frame != 0,
            bytes_per_line: size(raw.bytes_per_line, "negative bytes per line")?,
            pixels_per_line: size(raw.pixels_per_line, "negative pixels per line")?,
            lines: match raw.lines {
                -1 => None,
                lines => Some(size(lines, "negative number of lines")?),
            },
            depth: u32::try_from(raw.depth).map_err(|_| Error::Protocol("negative depth"))?,
        })
    }

    /// The number of bytes holding the samples of one line, without padding.
    fn packed_line_len(&self) -> usize {
        line_len(self.pixels_per_line, self.format.channels(), self.depth)
    }
}

/// A complete scanned image: the raw bytes of every line, and the parameters
/// that describe them.
///
/// Lines are [`Parameters::bytes_per_line`] bytes apart, and
/// [`Parameters::lines`] is always known. Three-pass scans are merged into a
/// single [`FrameFormat::Rgb`] frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The parameters of the image.
    pub parameters: Parameters,
    /// The image data, `bytes_per_line * lines` bytes.
    pub data: Vec<u8>,
}

impl Frame {
    /// The frame for `data` read from a single-pass scan: the line count is
    /// the number of complete lines read (a trailing partial line is
    /// dropped).
    pub(crate) fn from_single_pass(mut parameters: Parameters, mut data: Vec<u8>) -> Result<Self> {
        if parameters.bytes_per_line == 0 {
            data.clear();
            parameters.lines = Some(0);
            return Ok(Self { parameters, data });
        }
        if parameters.bytes_per_line < parameters.packed_line_len() {
            return Err(Error::Protocol(
                "bytes per line smaller than the pixels of a line",
            ));
        }
        let lines = data.len() / parameters.bytes_per_line;
        data.truncate(lines * parameters.bytes_per_line);
        parameters.lines = Some(lines);
        Ok(Self { parameters, data })
    }

    /// Merges the red, green and blue frames of a three-pass scan, in any
    /// order, into one RGB frame.
    pub(crate) fn merge_three_pass(frames: Vec<Frame>) -> Result<Self> {
        let find = |format| {
            frames
                .iter()
                .find(|frame| frame.parameters.format == format)
                .ok_or(Error::Protocol(
                    "three-pass scan without a red, green and blue frame",
                ))
        };
        let channels = [
            find(FrameFormat::Red)?,
            find(FrameFormat::Green)?,
            find(FrameFormat::Blue)?,
        ];
        let first = channels[0].parameters;
        if channels.iter().any(|frame| {
            frame.parameters.pixels_per_line != first.pixels_per_line
                || frame.parameters.depth != first.depth
        }) {
            return Err(Error::Protocol("three-pass frames of different sizes"));
        }
        let depth = first.depth;
        if !matches!(depth, 1 | 8 | 16) {
            return Err(Error::UnsupportedFrame {
                format: FrameFormat::Rgb,
                depth,
            });
        }
        let lines = channels
            .iter()
            .map(|frame| frame.parameters.lines.unwrap_or(0))
            .min()
            .unwrap_or(0);
        let width = first.pixels_per_line;
        let parameters = Parameters {
            format: FrameFormat::Rgb,
            last_frame: true,
            bytes_per_line: line_len(width, 3, depth),
            pixels_per_line: width,
            lines: Some(lines),
            depth,
        };
        let mut data = vec![0; parameters.bytes_per_line * lines];
        for line in 0..lines {
            let output = &mut data[line * parameters.bytes_per_line..][..parameters.bytes_per_line];
            for (channel, frame) in channels.iter().enumerate() {
                let input = &frame.data[line * frame.parameters.bytes_per_line..];
                for x in 0..width {
                    match depth {
                        8 => output[x * 3 + channel] = input[x],
                        16 => output[(x * 3 + channel) * 2..][..2]
                            .copy_from_slice(&input[x * 2..][..2]),
                        _ => {
                            if bit(input, x, 0, 1) {
                                output[x / 8 * 3 + channel] |= 0x80 >> (x % 8);
                            }
                        }
                    }
                }
            }
        }
        Ok(Self { parameters, data })
    }

    /// The width of the image in pixels.
    pub fn width(&self) -> usize {
        self.parameters.pixels_per_line
    }

    /// The height of the image in lines.
    pub fn height(&self) -> usize {
        self.parameters.lines.unwrap_or(0)
    }

    /// The samples of line `y`, without the padding at the end of the line.
    ///
    /// # Panics
    ///
    /// Panics if `y` is not less than [`height`](Self::height).
    pub fn line(&self, y: usize) -> &[u8] {
        assert!(y < self.height(), "line {y} out of range");
        &self.data[y * self.parameters.bytes_per_line..][..self.parameters.packed_line_len()]
    }

    /// Converts the frame to an [`image::DynamicImage`].
    ///
    /// Gray and RGB frames are supported, at depths 1 (expanded to 8-bit
    /// samples), 8 and 16. Following the SANE standard, a 1-bit gray sample
    /// of 1 is black, while a 1-bit RGB sample of 1 is full intensity.
    #[cfg(feature = "image")]
    #[cfg_attr(docsrs, doc(cfg(feature = "image")))]
    pub fn to_image(&self) -> Result<image::DynamicImage> {
        use image::{DynamicImage, ImageBuffer};

        let Parameters { format, depth, .. } = self.parameters;
        let unsupported = Error::UnsupportedFrame { format, depth };
        let channels = match format {
            FrameFormat::Gray | FrameFormat::Rgb => format.channels(),
            _ => return Err(unsupported),
        };
        let (width, height) = (self.width(), self.height());
        let (Ok(image_width), Ok(image_height)) = (u32::try_from(width), u32::try_from(height))
        else {
            return Err(Error::Protocol("image too large"));
        };
        let samples = width * channels;
        let too_large = || Error::Protocol("image too large");
        match depth {
            1 | 8 => {
                let mut buffer = Vec::with_capacity(samples * height);
                for y in 0..height {
                    let line = self.line(y);
                    if depth == 8 {
                        buffer.extend_from_slice(&line[..samples]);
                    } else {
                        // A gray bit of 1 is black; an RGB bit of 1 is full intensity.
                        let on = if format == FrameFormat::Gray { 0 } else { 255 };
                        buffer.extend((0..samples).map(|i| {
                            if bit(line, i / channels, i % channels, channels) {
                                on
                            } else {
                                255 - on
                            }
                        }));
                    }
                }
                Ok(if channels == 1 {
                    DynamicImage::ImageLuma8(
                        ImageBuffer::from_raw(image_width, image_height, buffer)
                            .ok_or_else(too_large)?,
                    )
                } else {
                    DynamicImage::ImageRgb8(
                        ImageBuffer::from_raw(image_width, image_height, buffer)
                            .ok_or_else(too_large)?,
                    )
                })
            }
            16 => {
                let mut buffer = Vec::with_capacity(samples * height);
                for y in 0..height {
                    let line = self.line(y);
                    buffer.extend(
                        line[..samples * 2]
                            .chunks_exact(2)
                            .map(|sample| u16::from_ne_bytes([sample[0], sample[1]])),
                    );
                }
                Ok(if channels == 1 {
                    DynamicImage::ImageLuma16(
                        ImageBuffer::from_raw(image_width, image_height, buffer)
                            .ok_or_else(too_large)?,
                    )
                } else {
                    DynamicImage::ImageRgb16(
                        ImageBuffer::from_raw(image_width, image_height, buffer)
                            .ok_or_else(too_large)?,
                    )
                })
            }
            _ => Err(unsupported),
        }
    }
}

/// The number of bytes of a line of `width` pixels, without padding.
///
/// 1-bit samples are packed 8 pixels per byte, most significant bit first,
/// and with several channels each group of 8 pixels takes one byte per
/// channel: red, green, then blue.
fn line_len(width: usize, channels: usize, depth: u32) -> usize {
    if depth == 1 {
        width.div_ceil(8) * channels
    } else {
        (width * channels * depth as usize).div_ceil(8)
    }
}

/// The 1-bit sample of `channel` for pixel `x` of a line with `channels`
/// channels (see [`line_len`]).
fn bit(line: &[u8], x: usize, channel: usize, channels: usize) -> bool {
    line[x / 8 * channels + channel] & (0x80 >> (x % 8)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parameters(
        format: FrameFormat,
        width: usize,
        depth: u32,
        bytes_per_line: usize,
    ) -> Parameters {
        Parameters {
            format,
            last_frame: false,
            bytes_per_line,
            pixels_per_line: width,
            lines: None,
            depth,
        }
    }

    #[test]
    fn parameters_from_raw() {
        let raw = sys::SANE_Parameters {
            format: sys::SANE_FRAME_RGB,
            last_frame: 1,
            bytes_per_line: 30,
            pixels_per_line: 10,
            lines: -1,
            depth: 8,
        };
        let params = Parameters::from_raw(&raw).unwrap();
        assert_eq!(params.format, FrameFormat::Rgb);
        assert!(params.last_frame);
        assert_eq!(params.lines, None);
        let bad = sys::SANE_Parameters {
            bytes_per_line: -3,
            ..raw
        };
        assert!(matches!(
            Parameters::from_raw(&bad),
            Err(Error::Protocol(_))
        ));
        let other = sys::SANE_Parameters {
            format: 0x0f,
            lines: 4,
            ..raw
        };
        let params = Parameters::from_raw(&other).unwrap();
        assert_eq!(params.format, FrameFormat::Unknown(0x0f));
        assert_eq!(params.lines, Some(4));
    }

    #[test]
    fn single_pass_counts_complete_lines() {
        let frame = Frame::from_single_pass(
            parameters(FrameFormat::Gray, 2, 8, 3),
            vec![1, 2, 0, 3, 4, 0, 5],
        )
        .unwrap();
        assert_eq!(frame.height(), 2);
        assert_eq!(frame.data.len(), 6);
        assert_eq!(frame.line(1), [3, 4]);
        assert!(Frame::from_single_pass(parameters(FrameFormat::Rgb, 2, 8, 3), vec![]).is_err());
    }

    fn channel(
        format: FrameFormat,
        depth: u32,
        data: Vec<u8>,
        width: usize,
        bytes_per_line: usize,
    ) -> Frame {
        let mut params = parameters(format, width, depth, bytes_per_line);
        params.lines = Some(data.len() / bytes_per_line);
        Frame {
            parameters: params,
            data,
        }
    }

    #[test]
    fn three_pass_8_bit_merge_in_any_order() {
        let frames = vec![
            channel(FrameFormat::Blue, 8, vec![5, 6], 2, 2),
            channel(FrameFormat::Red, 8, vec![1, 2], 2, 2),
            channel(FrameFormat::Green, 8, vec![3, 4], 2, 2),
        ];
        let merged = Frame::merge_three_pass(frames).unwrap();
        assert_eq!(merged.parameters.format, FrameFormat::Rgb);
        assert_eq!(merged.parameters.bytes_per_line, 6);
        assert_eq!(merged.data, [1, 3, 5, 2, 4, 6]);
    }

    #[test]
    fn three_pass_16_bit_and_1_bit_merge() {
        let frames = vec![
            channel(FrameFormat::Red, 16, vec![1, 0], 1, 2),
            channel(FrameFormat::Green, 16, vec![2, 0], 1, 2),
            channel(FrameFormat::Blue, 16, vec![3, 0], 1, 2),
        ];
        assert_eq!(
            Frame::merge_three_pass(frames).unwrap().data,
            [1, 0, 2, 0, 3, 0]
        );
        // Pixels: red = 1 0 1, green = 0 1 1, blue = 0 0 1 (3 pixels, 1 byte each).
        let frames = vec![
            channel(FrameFormat::Red, 1, vec![0b1010_0000], 3, 1),
            channel(FrameFormat::Green, 1, vec![0b0110_0000], 3, 1),
            channel(FrameFormat::Blue, 1, vec![0b0010_0000], 3, 1),
        ];
        let merged = Frame::merge_three_pass(frames).unwrap();
        // One byte per channel for each group of 8 pixels.
        assert_eq!(merged.parameters.bytes_per_line, 3);
        assert_eq!(merged.data, [0b1010_0000, 0b0110_0000, 0b0010_0000]);
    }

    #[test]
    fn three_pass_needs_every_channel() {
        let frames = vec![
            channel(FrameFormat::Red, 8, vec![1], 1, 1),
            channel(FrameFormat::Green, 8, vec![1], 1, 1),
        ];
        assert!(matches!(
            Frame::merge_three_pass(frames),
            Err(Error::Protocol(_))
        ));
    }

    #[cfg(feature = "image")]
    #[test]
    fn image_conversion() {
        let gray1 = channel(FrameFormat::Gray, 1, vec![0b1000_0000], 2, 1);
        let image = gray1.to_image().unwrap().into_luma8();
        assert_eq!(image.as_raw(), &[0, 255]);

        // Two pixels: red, then cyan.
        let rgb1 = channel(
            FrameFormat::Rgb,
            1,
            vec![0b1000_0000, 0b0100_0000, 0b0100_0000],
            2,
            3,
        );
        assert_eq!(
            rgb1.to_image().unwrap().into_rgb8().as_raw(),
            &[255, 0, 0, 0, 255, 255]
        );

        let gray16 = channel(
            FrameFormat::Gray,
            16,
            0x1234u16.to_ne_bytes().to_vec(),
            1,
            2,
        );
        assert_eq!(gray16.to_image().unwrap().into_luma16().as_raw(), &[0x1234]);

        let rgb8 = channel(FrameFormat::Rgb, 8, vec![1, 2, 3, 0], 1, 4);
        let image = rgb8.to_image().unwrap();
        assert_eq!((image.width(), image.height()), (1, 1));
        assert_eq!(image.into_rgb8().as_raw(), &[1, 2, 3]);

        let red = channel(FrameFormat::Red, 8, vec![1], 1, 1);
        assert!(matches!(
            red.to_image(),
            Err(Error::UnsupportedFrame { .. })
        ));
    }
}
