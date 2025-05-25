use std::{collections, convert::TryInto, time::Instant};

type HashMap<A, B> = collections::HashMap<A, B, ahash::RandomState>;
type EncodingKeyVec = tinyvec::TinyVec<[EncodingKey; 1]>;

use anyhow::{Context, Result, bail, ensure};
use bytes::Buf;

use crate::{ContentKey, EncodingKey};

pub(crate) struct Encoding {
    _especs: Vec<String>,
    c2e: Vec<(u128, u128, u64)>,
    e2i: Vec<(u128, u32, u64)>,
    //cmap: HashMap<ContentKey, (EncodingKey, u64)>,
    cmap_extra: HashMap<ContentKey, EncodingKeyVec>,
    _espec: String,
}

impl Encoding {
    pub(crate) fn c2e(&self, c: ContentKey) -> Result<EncodingKey> {
        let found = self.c2e.binary_search_by_key(&c.0, |&(a, _b, _c)| a);
        if let Ok(found) = found {
            Ok(EncodingKey(self.c2e[found].1))
        } else {
            bail!("no encoding key for content key {}", c)
        }
    }
}

impl std::fmt::Display for Encoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encoding")
            .field("c2e_len", &self.c2e.len())
            .finish()
    }
}

impl std::fmt::Debug for Encoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

// FIXME: encoding is sorted so we can mmap it in and use it as a data structure directly to avoid a time consuming parse step
// binary search

#[tracing::instrument(err, skip(data))]
pub(crate) fn parse(data: &[u8]) -> Result<Encoding> {
    let start = Instant::now();
    tracing::debug!("Parsing encoding data");
    let mut p = data;
    ensure!(p.remaining() >= 16, "truncated encoding header");
    ensure!(&p.get_u16().to_be_bytes() == b"EN", "not encoding format");
    ensure!(p.get_u8() == 1, "unsupported encoding version");
    ensure!(p.get_u8() == 16, "unsupported ckey hash size");
    ensure!(p.get_u8() == 16, "unsupported ekey hash size");
    let cpagekb: usize = p.get_u16().into();
    let epagekb: usize = p.get_u16().into();
    let ccount: usize = p.get_u32().try_into()?;
    let ecount: usize = p.get_u32().try_into()?;
    ensure!(p.get_u8() == 0, "unexpected nonzero byte in header");
    let espec_size = p.get_u32().try_into()?;
    ensure!(p.remaining() >= espec_size, "truncated espec table");
    let especs = p[0..espec_size]
        .split(|b| *b == 0)
        .map(|s| String::from_utf8(s.to_vec()).context("parsing encoding espec"))
        .collect::<Result<Vec<String>>>()?;
    p.advance(espec_size);
    ensure!(p.remaining() >= ccount * 32);
    let mut cpages = Vec::<(ContentKey, u128)>::with_capacity(ccount);
    for _ in 0..ccount {
        cpages.push((ContentKey(p.get_u128()), p.get_u128()));
    }
    let assumed_hash_count = (ccount * cpagekb * 1024) / 32;
    let mut c2e = Vec::with_capacity(assumed_hash_count);
    let mut cmap_extra = HashMap::<ContentKey, EncodingKeyVec>::default();
    for (first_key, hash) in cpages {
        let pagesize = cpagekb * 1024;
        #[cfg(debug_assertions)]
        ensure!(
            hash == crate::md5hash(&p[0..pagesize]),
            "content page checksum"
        );
        let mut page = p.take(pagesize);
        let mut first = true;
        while page.remaining() >= 22 && page.chunk()[0] != b'0' {
            let key_count = page.get_u8().into();
            let file_size = (u64::from(page.get_u8()) << 32) | u64::from(page.get_u32());
            let ckey = ContentKey(page.get_u128());
            #[cfg(debug_assertions)]
            ensure!(!first || first_key == ckey, "first key mismatch in content");
            first = false;
            #[cfg(debug_assertions)]
            ensure!(page.remaining() >= key_count * 16_usize);

            if key_count > 0 {
                c2e.push((ckey.0, page.get_u128(), file_size));
                //cmap.insert(ckey, (EncodingKey(page.get_u128()), file_size));
                if key_count > 1 {
                    let mut ekeys = EncodingKeyVec::with_capacity(key_count);
                    for _ in 1..key_count {
                        ekeys.push(EncodingKey(page.get_u128()));
                    }
                    //tracing::error!("{ckey} Multiple ekeys {ekeys}");
                    cmap_extra.insert(ckey, ekeys);
                }
            }
        }
        p.advance(pagesize)
    }
    ensure!(p.remaining() >= ecount * 32);
    let mut epages = Vec::<(u128, u128)>::with_capacity(ecount);
    for _ in 0..ecount {
        epages.push((p.get_u128(), p.get_u128()));
    }

    let (e2i, p) = build_e2i(&epages, epagekb, p)?;
    let espec = String::from_utf8(p.to_vec())?;
    tracing::info!(
        espec = espec,
        especs_len = especs.len(),
        e2i_len = e2i.len(),
        c2e_len = c2e.len(),
        duration = (Instant::now() - start).as_secs_f32(),
    );
    Ok(Encoding {
        _especs: especs,
        c2e,
        //cmap,
        cmap_extra,
        e2i,
        _espec: espec,
    })
}

#[allow(clippy::type_complexity)] // complex return type only used once to split up a function
fn build_e2i<'a>(
    epages: &[(u128, u128)],
    epagekb: usize,
    mut p: &'a [u8],
) -> Result<(Vec<(u128, u32, u64)>, &'a [u8])> {
    let mut e2i = Vec::with_capacity((epages.len() * epagekb * 1024) / 32);

    for &(first_key, hash) in epages {
        let pagesize = epagekb * 1024;
        #[cfg(debug_assertions)]
        ensure!(
            hash == crate::md5hash(&p[0..pagesize]),
            "encoding page checksum"
        );
        let mut page = p.take(pagesize);
        let mut first = true;
        while page.remaining() >= 25 && page.chunk()[0] != b'0' {
            let ekey = page.get_u128();
            let index = page.get_u32();
            let file_size = (u64::from(page.get_u8()) << 32) | u64::from(page.get_u32());
            if first {
                #[cfg(debug_assertions)]
                ensure!(first_key == ekey, "first key mismatch in content");
                first = false;
            }
            e2i.push((ekey, index, file_size));
            //emap.insert(ekey, (index, file_size));
        }
        p.advance(pagesize)
    }

    Ok((e2i, p))
}
