`sane/sane.h` is an unmodified copy of the header shipped by sane-backends
1.3.1 (public domain, see its notice). The build script only uses it on
docs.rs, which has no libsane: every other build reads the system header
found through pkg-config.
