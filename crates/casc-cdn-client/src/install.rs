use std::io::{BufRead, Read};

type EncodingKeyVec = tinyvec::TinyVec<[EncodingKey; 1]>;

use anyhow::{Result, ensure};
use bytes::Buf;

use crate::EncodingKey;

#[derive(Debug)]
pub struct InstallFile {
    pub name: String,
    pub key: crate::Key,
}

#[derive(Debug)]
pub(crate) struct Install {
    pub root_names: Vec<String>,
    pub files: Vec<InstallFile>,
}

impl Install {}

impl std::fmt::Display for Install {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Install")
            .field("root_names", &self.root_names)
            .finish()
    }
}
#[derive(Debug, Clone)]
struct Tag {
    name: String,
    ty: u16,
    mask: Vec<u8>,
}

#[tracing::instrument(err, skip(data))]
pub(crate) fn parse(data: &[u8]) -> Result<Install> {
    tracing::info!("Parsing install data");
    let mut p = data;
    ensure!(p.remaining() >= 16, "truncated encoding header");
    ensure!(&p.get_u16().to_be_bytes() == b"IN", "not install format");
    ensure!(p.get_u8() == 1, "unsupported install version");
    let _unk = p.get_u8(); // unk
    //dbg!(unk);
    let num_tags = p.get_u16();
    let num_files = p.get_u32();
    let num_mask_bytes = num_files.div_ceil(8);

    let mut root_names = vec![];
    let mut files = vec![];

    let mut tags = vec![];
    let mut mask_bytes_buf = vec![0u8; num_mask_bytes as usize];
    for _i in 0..num_tags {
        let mut name_vec = vec![];
        let _name_len = p.read_until(b'\0', &mut name_vec)?;
        name_vec.pop();
        let tag_name = String::from_utf8_lossy(&name_vec).into_owned();
        let ty = p.get_u16();
        p.read_exact(&mut mask_bytes_buf)?;

        // dbg!(_tag_name, _ty);
        tags.push(Tag {
            name: tag_name,
            ty,
            mask: mask_bytes_buf.clone(),
        })
    }

    let needed_tags = tags
        .iter()
        .filter(|t| t.name == "Windows" || t.name == "x86_64" || t.name == "US")
        .cloned()
        .collect::<Vec<_>>();

    // FIXME: preserve tag info in Install and make tag selection generic
    // instead of hardcoding it here
    for i in 0..num_files {
        let mut name_vec = vec![];
        let _name_len = p.read_until(b'\0', &mut name_vec)?;
        name_vec.pop();
        let file_name = String::from_utf8_lossy(&name_vec).into_owned();
        let md5 = crate::Key(p.get_u128());
        let size = p.get_u32();
        if !file_name.contains('\\') {
            root_names.push(file_name.clone());
        }
        if tracing::enabled!(tracing::Level::DEBUG) && file_name.contains(".exe") {
            let mut tagged = "".to_string();
            for tag in &tags {
                if (tag.mask[i as usize / 8] & (1 << (i % 8))) != 0 {
                    if !tagged.is_empty() {
                        tagged += " ";
                    }
                    tagged += &tag.name;
                }
            }
            tracing::debug!("{tagged}\n{file_name}, {md5}, {}", size / (1024 * 1024))
        }
        // files.retain(|x: &InstallFile| x.name != file_name);
        let mut has_needed_tags = true;
        for tag in &needed_tags {
            if (tag.mask[i as usize / 8] & (1 << (i % 8))) == 0 {
                has_needed_tags = false;
                break;
            }
        }
        if has_needed_tags {
            files.push(InstallFile {
                name: file_name,
                key: md5,
            });
        }
    }

    root_names.sort_unstable();
    // files.sort_unstable_by_key(|x| &x.name); // lifetime error, ref lasts too long
    files.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    Ok(Install { root_names, files })
}
