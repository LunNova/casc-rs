use anyhow::{Result, anyhow, bail, ensure};
use bytes::Buf;
use std::convert::TryInto;

fn parse_blte_chunk(data: &[u8], output_buffer: &mut [u8]) -> Result<()> {
    use miniz_oxide::inflate;
    let chunk_data = &data[1..];
    match data[0] {
        b'N' => {
            ensure!(chunk_data.len() == output_buffer.len());
            output_buffer.copy_from_slice(chunk_data);
        }
        b'Z' => {
            let size = inflate::decompress_slice_iter_to_slice(
                output_buffer,
                std::iter::once(chunk_data),
                true,
                cfg!(not(debug_assertions)),
            )
            .map_err(|s| anyhow!(format!("inflate error {:?}", s)))?;
            ensure!(size == output_buffer.len());
        }
        _ => bail!("invalid encoding"),
    };
    Ok(())
}

pub(crate) fn parse(checksum: u128, data: &[u8]) -> Result<Vec<u8>> {
    let mut p = data;
    ensure!(p.remaining() >= 12, "truncated header");
    ensure!(&p.get_u32().to_be_bytes() == b"BLTE", "not BLTE format");
    let header_size = p.get_u32().try_into()?;
    if header_size == 0 {
        bail!("missing BLTE header not supported");
        // ensure!(crate::md5hash(data) == checksum);
        // return Ok(parse_blte_chunk(p)?.to_vec());
    }
    ensure!(p.remaining() >= header_size - 8);
    ensure!(
        crate::md5hash(&data[0..header_size]) == checksum,
        "header checksum error"
    );
    ensure!(p.get_u8() == 0xf, "bad flag byte");
    let chunk_count: usize = ((u32::from(p.get_u8()) << 16) | u32::from(p.get_u16())).try_into()?;
    ensure!(header_size == chunk_count * 24 + 12, "header size mismatch");
    let mut chunkinfo = Vec::<(usize, usize, u128)>::new();
    let mut overall_uncompressed_size = 0;
    for _ in 0..chunk_count {
        let compressed_size = p.get_u32().try_into()?;
        let uncompressed_size = p.get_u32().try_into()?;
        let checksum = p.get_u128();
        chunkinfo.push((compressed_size, uncompressed_size, checksum));
        overall_uncompressed_size += uncompressed_size;
    }
    let mut result = vec![0u8; overall_uncompressed_size];
    let mut result_ptr = 0;
    for (compressed_size, uncompressed_size, checksum) in chunkinfo {
        let chunk = &p[0..compressed_size];
        #[cfg(debug_assertions)]
        ensure!(checksum == crate::md5hash(chunk), "chunk checksum error");
        parse_blte_chunk(
            chunk,
            &mut result[result_ptr..result_ptr + uncompressed_size],
        )?;
        result_ptr += uncompressed_size;
        //ensure!(data.len() == uncompressed_size, "invalid uncompressed size");
        p.advance(compressed_size)
    }
    ensure!(!p.has_remaining(), "trailing blte data");
    Ok(result)
}
