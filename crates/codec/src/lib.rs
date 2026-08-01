//! Safe wrappers over the C compression libraries that CPython itself links.
//!
//! The NCD algorithms measure the compressed length of their input, and the
//! port must reproduce CPython's lengths exactly. The only way to guarantee
//! that is to call the very same C libraries with the very same settings, so
//! these wrappers bind `libbz2`, `libz` and `liblzma` directly. This crate is
//! the single place in the workspace that touches C code; the unsafe blocks
//! below are minimal, buffer-safety reviewed, and the only ones in the repo.

/// bzip2-compress `data` the same way `bz2.compress(data, 9)` does, and drop
/// the 15-byte bz2 header, mirroring `BZ2NCD._compress`.
///
/// CPython's one-shot `bz2.compress` uses the same streaming calls with the
/// same settings (block size 9, verbosity 0, work factor 0), so the output is
/// byte-identical.
pub fn bz2_compress(data: &[u8]) -> Vec<u8> {
    use std::os::raw::{c_char, c_int, c_uint};
    let bound = data.len() + data.len() / 100 + 601;
    let mut stream: bzip2_sys::bz_stream = unsafe {
        // SAFETY: `bz_stream` is plain data; zero-initializing is what the C
        // API expects before `BZ2_bzCompressInit` fills it.
        std::mem::zeroed()
    };
    let init = unsafe {
        // SAFETY: `stream` is a valid, zeroed `bz_stream`.
        bzip2_sys::BZ2_bzCompressInit(&mut stream, 9, 0, 0)
    };
    assert_eq!(init, bzip2_sys::BZ_OK as c_int, "bzip2 init failed");
    let mut dest = vec![0u8; bound];
    stream.next_in = data.as_ptr() as *mut c_char;
    stream.avail_in = data.len() as c_uint;
    stream.next_out = dest.as_mut_ptr() as *mut c_char;
    stream.avail_out = bound as c_uint;
    let ret = unsafe {
        // SAFETY: the in/out pointers and avail counts were set above; `dest`
        // is the bzip2 worst-case bound for `data`, so one BZ_FINISH pass
        // produces the whole stream.
        bzip2_sys::BZ2_bzCompress(&mut stream, bzip2_sys::BZ_FINISH)
    };
    unsafe {
        // SAFETY: frees the internal state allocated by `BZ2_bzCompressInit`.
        bzip2_sys::BZ2_bzCompressEnd(&mut stream)
    };
    assert_eq!(
        ret,
        bzip2_sys::BZ_STREAM_END as c_int,
        "bzip2 compression failed"
    );
    let used = bound - stream.avail_out as usize;
    dest.truncate(used);
    dest.into_iter().skip(15).collect()
}

/// zlib-compress `data` the same way `zlib.compress(data)` does, and drop the
/// 2-byte zlib header, mirroring `ZLIBNCD._compress`.
pub fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let bound = unsafe {
        // SAFETY: `compressBound` only reads `data.len()` and returns the
        // upper bound on the compressed size; no pointers are involved.
        libz_sys::compressBound(data.len() as libz_sys::uLong)
    };
    let mut dest = vec![0u8; bound as usize];
    let mut dest_len = bound;
    let ret = unsafe {
        // SAFETY: `dest` is `bound` bytes long (compressBound of `data`),
        // `dest_len` is the capacity in/out, and both pointers are valid.
        libz_sys::compress2(
            dest.as_mut_ptr(),
            &mut dest_len,
            data.as_ptr(),
            data.len() as libz_sys::uLong,
            libz_sys::Z_DEFAULT_COMPRESSION,
        )
    };
    assert_eq!(ret, libz_sys::Z_OK, "zlib compression failed");
    dest.truncate(dest_len as usize);
    dest.into_iter().skip(2).collect()
}

/// lzma/xz-compress `data` the same way `lzma.compress(data)` does, and drop
/// the 14-byte xz header, mirroring `LZMANCD._compress`.
pub fn lzma_compress(data: &[u8]) -> Vec<u8> {
    let bound = unsafe {
        // SAFETY: `lzma_stream_buffer_bound` is a pure size computation.
        lzma_sys::lzma_stream_buffer_bound(data.len())
    };
    let mut dest = vec![0u8; bound];
    let mut options = unsafe {
        // SAFETY: `lzma_options_lzma` is plain data; zero-initializing is what
        // the C API expects before `lzma_lzma_preset` fills it.
        std::mem::zeroed()
    };
    let preset_ret = unsafe {
        // SAFETY: `options` is a valid pointer to `lzma_options_lzma`.
        lzma_sys::lzma_lzma_preset(&mut options, 6)
    };
    assert_ne!(preset_ret, 0, "lzma preset failed");
    let mut filters = [
        lzma_sys::lzma_filter {
            id: lzma_sys::LZMA_FILTER_LZMA2,
            options: &mut options as *mut _ as *mut std::os::raw::c_void,
        },
        lzma_sys::lzma_filter {
            id: lzma_sys::LZMA_VLI_UNKNOWN,
            options: std::ptr::null_mut(),
        },
    ];
    let mut out_pos = 0usize;
    let ret = unsafe {
        // SAFETY: `filters` is a null-terminated filter chain, `dest` is
        // `bound` bytes (the xz worst-case bound for `data`), and `out_pos`
        // is the capacity in/out. liblzma writes at most `bound` bytes.
        lzma_sys::lzma_stream_buffer_encode(
            filters.as_mut_ptr(),
            lzma_sys::LZMA_CHECK_CRC64,
            std::ptr::null(),
            data.as_ptr(),
            data.len(),
            dest.as_mut_ptr(),
            &mut out_pos,
            dest.len(),
        )
    };
    assert_eq!(ret, lzma_sys::LZMA_OK, "lzma compression failed");
    dest.truncate(out_pos);
    dest.into_iter().skip(14).collect()
}
