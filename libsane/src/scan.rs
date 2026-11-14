//! Reading the data of a scan.

use std::fmt;
use std::io;

use crate::Device;
use crate::error::{Error, Result};
use crate::frame::{Frame, FrameFormat, Parameters};

/// The largest buffer allocated up front for a frame (the parameters come
/// from the backend and may be wrong).
const MAX_PREALLOCATION: usize = 64 << 20;

/// A scan in progress, started by [`Device::start`].
///
/// Data is read frame by frame: [`read`](Self::read) (or the [`io::Read`]
/// implementation) returns the bytes of the current frame, then 0 at its end.
/// Single-pass scanners send one frame per image; three-pass scanners send a
/// red, a green and a blue frame, moved through with
/// [`next_frame`](Self::next_frame). [`read_image`](Self::read_image) does
/// all of this and returns the whole image.
///
/// With a document feeder, [`next_page`](Self::next_page) moves to the next
/// page without ending the batch.
///
/// Dropping the scan cancels it (`sane_cancel`), which the SANE standard
/// requires after every scan, complete or not.
pub struct Scan<'d, 's> {
    device: &'d mut Device<'s>,
    parameters: Parameters,
    frame_done: bool,
}

impl<'d, 's> Scan<'d, 's> {
    /// Wraps a started scan; cancels it if its parameters cannot be read.
    pub(crate) fn new(device: &'d mut Device<'s>) -> Result<Self> {
        match device.handle.parameters() {
            Ok(parameters) => Ok(Self {
                device,
                parameters,
                frame_done: false,
            }),
            Err(error) => {
                device.handle.cancel();
                Err(error)
            }
        }
    }

    /// The parameters of the current frame.
    pub fn parameters(&self) -> &Parameters {
        &self.parameters
    }

    /// Whether the current frame was read to its end.
    pub fn is_frame_done(&self) -> bool {
        self.frame_done
    }

    /// Reads data of the current frame into `buffer` (`sane_read`): the
    /// number of bytes read, 0 at the end of the frame (or if `buffer` is
    /// empty). Blocks until data is available.
    ///
    /// # Errors
    ///
    /// The error returned by the backend, for example [`Error::Jammed`] or
    /// [`Error::Cancelled`].
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        if buffer.is_empty() || self.frame_done {
            return Ok(0);
        }
        loop {
            match self.device.handle.read(buffer)? {
                None => {
                    self.frame_done = true;
                    return Ok(0);
                }
                // Blocking reads should not return without data; try again.
                Some(0) => continue,
                Some(len) => return Ok(len),
            }
        }
    }

    /// Reads the rest of the current frame.
    ///
    /// The returned frame has a known line count: the number of complete
    /// lines read.
    pub fn read_frame(&mut self) -> Result<Frame> {
        let expected = self
            .parameters
            .lines
            .and_then(|lines| lines.checked_mul(self.parameters.bytes_per_line))
            .unwrap_or(0);
        let mut data = Vec::with_capacity(expected.min(MAX_PREALLOCATION));
        let mut chunk = vec![0; 64 * 1024];
        loop {
            match self.read(&mut chunk)? {
                0 => break,
                len => data.extend_from_slice(&chunk[..len]),
            }
        }
        Frame::from_single_pass(self.parameters, data)
    }

    /// Starts the next frame of a three-pass scan: returns `false` if the
    /// current frame is the last of the image.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidState`] if the current frame was not read to its end,
    /// or the error returned by the backend.
    pub fn next_frame(&mut self) -> Result<bool> {
        if !self.frame_done {
            return Err(Error::InvalidState(
                "the current frame was not read to its end",
            ));
        }
        if self.parameters.last_frame {
            return Ok(false);
        }
        self.restart()?;
        Ok(true)
    }

    /// Starts scanning the next page of a document feeder, without cancelling
    /// the batch: returns `false` when the feeder is empty
    /// ([`Error::NoDocs`]).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidState`] if the current image was not read to its end,
    /// or the error returned by the backend.
    pub fn next_page(&mut self) -> Result<bool> {
        if !self.frame_done || !self.parameters.last_frame {
            return Err(Error::InvalidState(
                "the current image was not read to its end",
            ));
        }
        match self.restart() {
            Ok(()) => Ok(true),
            Err(Error::NoDocs) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn restart(&mut self) -> Result<()> {
        self.device.handle.start()?;
        self.parameters = self.device.handle.parameters()?;
        self.frame_done = false;
        Ok(())
    }

    /// Reads the rest of the current image: every remaining frame, merged
    /// into one RGB frame for three-pass scans.
    ///
    /// The scan is not ended, so [`next_page`](Self::next_page) can follow.
    pub fn read_image(&mut self) -> Result<Frame> {
        let mut first = self.read_frame()?;
        // A gray or RGB frame is a whole image, whatever `last_frame` says:
        // starting again would scan another page.
        if !is_single_channel(first.parameters.format) {
            first.parameters.last_frame = true;
            return Ok(first);
        }
        let mut frames = vec![first];
        while frames.len() < 3 && self.next_frame()? {
            frames.push(self.read_frame()?);
        }
        Frame::merge_three_pass(frames)
    }

    /// Cancels the scan (`sane_cancel`). Dropping the scan does the same.
    pub fn cancel(self) {}
}

/// Whether frames of this format are one channel of a three-pass scan.
fn is_single_channel(format: FrameFormat) -> bool {
    matches!(
        format,
        FrameFormat::Red | FrameFormat::Green | FrameFormat::Blue
    )
}

impl io::Read for Scan<'_, '_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        Scan::read(self, buffer).map_err(io::Error::from)
    }
}

impl Drop for Scan<'_, '_> {
    fn drop(&mut self) {
        self.device.handle.cancel();
    }
}

impl fmt::Debug for Scan<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scan")
            .field("device", &self.device.name())
            .field("parameters", &self.parameters)
            .field("frame_done", &self.frame_done)
            .finish()
    }
}
