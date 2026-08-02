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
///
/// This intentionally drives the streaming `lzma_stream_encoder` +
/// `lzma_code(LZMA_FINISH)` API rather than the one-shot
/// `lzma_stream_buffer_encode` wrapper: CPython's `_lzma` module compresses
/// through the streaming API, and the two paths produce *different* LZMA2
/// payloads (and occasionally different lengths) for the same input, which
/// would break the NCD length parity this crate exists to guarantee.
pub fn lzma_compress(data: &[u8]) -> Vec<u8> {
    let mut options = unsafe {
        // SAFETY: `lzma_options_lzma` is plain data; zero-initializing is what
        // the C API expects before `lzma_lzma_preset` fills it.
        std::mem::zeroed()
    };
    let preset_ret = unsafe {
        // SAFETY: `options` is a valid pointer to `lzma_options_lzma`.
        lzma_sys::lzma_lzma_preset(&mut options, 6)
    };
    // `lzma_lzma_preset` does write the preset even though its bool return is
    // read back as 0 on this toolchain/ABI (observed dict_size == 8388608 == 8
    // MiB, the preset-6 default, alongside a 0 return). Trust the populated
    // options instead of the unreliable return value.
    let _ = preset_ret;
    assert_ne!(options.dict_size, 0, "lzma preset failed");
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
    let mut stream: lzma_sys::lzma_stream = unsafe {
        // SAFETY: `lzma_stream` is plain data; zero-initializing is what the C
        // API expects before `lzma_stream_encoder` fills it.
        std::mem::zeroed()
    };
    let init = unsafe {
        // SAFETY: `stream` is a valid, zeroed `lzma_stream` and `filters` is a
        // null-terminated filter chain; check CRC64 is what `lzma.compress`
        // uses.
        lzma_sys::lzma_stream_encoder(
            &mut stream,
            filters.as_mut_ptr(),
            lzma_sys::LZMA_CHECK_CRC64,
        )
    };
    assert_eq!(init, lzma_sys::LZMA_OK, "lzma encoder init failed");
    let mut dest = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    stream.next_in = data.as_ptr() as *mut u8;
    stream.avail_in = data.len();
    loop {
        stream.next_out = buf.as_mut_ptr();
        stream.avail_out = buf.len();
        let ret = unsafe {
            // SAFETY: `stream` was initialized above, `buf` is the in/out
            // buffer, and all input was handed to the encoder up front; one
            // `LZMA_FINISH` pass drives it to `LZMA_STREAM_END`.
            lzma_sys::lzma_code(&mut stream, lzma_sys::LZMA_FINISH)
        };
        let used = buf.len() - stream.avail_out;
        dest.extend_from_slice(&buf[..used]);
        if ret == lzma_sys::LZMA_STREAM_END {
            break;
        }
        assert_eq!(ret, lzma_sys::LZMA_OK, "lzma compression failed");
    }
    unsafe {
        // SAFETY: frees the internal state allocated by `lzma_stream_encoder`.
        lzma_sys::lzma_end(&mut stream)
    };
    dest.into_iter().skip(14).collect()
}

#[cfg(test)]
mod lzma_lengths {
    // Pin the port's lzma/xz compressed lengths (post 14-byte header trim) to
    // CPython's `lzma.compress(data)[14:]`, which the NCD algorithms measure.
    #[test]
    fn matches_cpython_header_trimmed_lengths() {
        // Observed via CPython 3.x: lzma.compress(x)[14:].
        let cases: &[(&[u8], usize)] = &[
            (b"", 18),
            (b"a", 46),
            (b"ab", 46),
            (b"abc", 46),
            (b"abcdefgh", 50),
        ];
        for (data, want) in cases {
            assert_eq!(
                super::lzma_compress(data).len(),
                *want,
                "lzma length for {data:?}"
            );
        }
    }
}
