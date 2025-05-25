pub(crate) fn md5hash(p: &[u8]) -> u128 {
    use md5::{Digest, Md5, digest::FixedOutput};
    let mut hasher = Md5::new();
    hasher.update(p);
    u128::from_be_bytes(hasher.finalize_fixed().into())
}

use derive_more::Display;
use reqwest::{Error, blocking::Response};

#[derive(Clone, Copy, Debug, Display, Eq, Hash, PartialEq)]
#[display("{:032x}", _0)]
pub(crate) struct ArchiveKey(pub(crate) u128);

#[derive(Clone, Copy, Debug, Display, Eq, Hash, PartialEq)]
#[display("{:032x}", _0)]
pub(crate) struct ContentKey(pub(crate) u128);

#[derive(Clone, Copy, Debug, Display, Eq, Hash, PartialEq, Default)]
#[display("{:032x}", _0)]
#[repr(transparent)]
pub(crate) struct EncodingKey(pub(crate) u128);

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) struct FileDataID(pub(crate) u32);

pub mod blte;
pub mod install;

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[tracing::instrument]
fn fetch(url: &str) -> Result<Response, Error> {
    tracing::debug!("Fetching");
    reqwest::blocking::ClientBuilder::new()
        // .http3_prior_knowledge()
        .user_agent(APP_USER_AGENT)
        .build()
        .unwrap()
        .get(url)
        // .version(reqwest::Version::HTTP_2)
        .send()
}

#[derive(Debug)]
struct CacheByKey {
    path: PathBuf,
}

impl CacheByKey {
    #[tracing::instrument(err, skip(self))]
    fn get(&self, url: &str, kind: &str, key: &str) -> Result<Vec<u8>> {
        tracing::info!("Retrieving {kind}/{key}");
        let formatted_key = format_hex_key(key);
        let mut keyed_path = self.path.join(kind);
        keyed_path.push(&formatted_key);
        let file = std::fs::read(&keyed_path);
        if let Ok(file) = file {
            tracing::debug!("Cache hit");
            return Ok(file);
        }
        tracing::debug!("Cache miss");
        let data = reqwest::blocking::ClientBuilder::new()
            // .http3_prior_knowledge()
            .user_agent(APP_USER_AGENT)
            .build()
            .unwrap()
            .get(url)
            .send()?
            .error_for_status()?
            .bytes()?;
        std::fs::create_dir_all(keyed_path.parent().unwrap())?;
        std::fs::write(keyed_path, &data)?;
        Ok(data.to_vec())
    }

    fn new(arg: impl AsRef<Path>) -> Self {
        Self {
            path: arg.as_ref().to_owned(),
        }
    }
}

struct PipeSeparatedVars {
    storage: String,
    headings: Vec<Range<usize>>,
    entries: Vec<Vec<Range<usize>>>,
    meta: String,
}

impl PipeSeparatedVars {
    fn headings(&self) -> impl Iterator<Item = &str> {
        self.headings.iter().map(|x| &self.storage[x.to_owned()])
    }

    fn entries(&self) -> impl Iterator<Item = impl Iterator<Item = &str>> {
        self.entries
            .iter()
            .map(|x| x.iter().map(|y| &self.storage[y.to_owned()]))
    }
}

impl Debug for PipeSeparatedVars {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipeSeparatedVars")
            //.field("storage", &self.storage)
            .field("meta", &self.meta)
            .field("headings", &self.headings().collect::<Vec<_>>())
            .field(
                "entries",
                &self
                    .entries()
                    .map(|x| x.collect::<Vec<_>>().join(" | "))
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

fn format_hex_key(hex: &str) -> String {
    format!("{}/{}/{hex}", &hex[0..2], &hex[2..4])
}

fn trimmed_index(backing: &str, needle: &str) -> Range<usize> {
    let needle = needle.trim();
    let start = unsafe { needle.as_ptr().byte_offset_from(backing.as_ptr()) } as usize;
    assert!(start < backing.len() && start + needle.len() < backing.len());
    start..start + needle.len()
}

fn load_pipe_separated_vars(backing: String) -> PipeSeparatedVars {
    let mut lines = backing.lines();
    let header = lines.next().unwrap();
    let headings: Vec<_> = header
        .split('|')
        .map(|x| trimmed_index(&backing, x))
        .collect();
    let mut meta = "".to_owned();
    let mut entries = vec![];

    for line in lines {
        let parts: Vec<_> = line
            .split('|')
            .map(|x| trimmed_index(&backing, x))
            .collect();

        if parts.len() != headings.len() || line.starts_with("##") {
            meta += line;
            meta += "\n";
        } else {
            entries.push(parts)
        }
    }

    PipeSeparatedVars {
        storage: backing,
        meta,
        headings,
        entries,
    }
}

fn pick_cdn(cdns: &PipeSeparatedVars) -> String {
    for ele in cdns.entries() {
        let ele = ele.collect::<Vec<_>>();
        if ele[0] == "us" {
            let mut https_only = ele[3].split(' ').filter(|x| x.contains("https://"));
            let mut https_only_blizzard = ele[3]
                .split(' ')
                .filter(|x| x.contains("https://"))
                .filter(|x| x.contains("cdn.blizzard.com"));
            let mut url = https_only_blizzard
                .next()
                .unwrap_or_else(|| https_only.next().unwrap());
            if let Some(idx) = url.find('?') {
                url = &url[0..idx];
            }
            return format!("{url}{}/", &ele[1]);
        }
    }

    panic!();
}

#[derive(Clone, Copy, Debug, Display, Eq, Hash, PartialEq)]
#[display("{:032x}", _0)]
pub struct Key(u128);

impl Key {
    fn from_str(s: &str) -> Result<Self> {
        let k = hex::decode(s)?;
        Ok(Self(u128::from_be_bytes(k.try_into().unwrap())))
    }

    fn as_hex_string(&self) -> String {
        hex::encode(self.0.to_be_bytes())
    }
}

struct FileKeys {
    ckey: Key,
    ekey: Key,
}

impl FileKeys {
    fn from_str(s: &str) -> Result<Self> {
        let mut split = s.split(' ');
        let ckey = Key::from_str(split.next().context("no ckey")?)?;
        let ekey = Key::from_str(split.next().context("no ckey")?)?;

        Ok(Self { ckey, ekey })
    }
}

impl Debug for FileKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{{c {} e {}}}",
            &self.ckey.as_hex_string(),
            &self.ekey.as_hex_string()
        )
    }
}

struct CascClient {
    cdn_prefix: String,
    encoding: encoding::Encoding,
    install: install::Install,
    cache: CacheByKey,
}

impl std::fmt::Debug for CascClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CascClient")
            .field("cdn_prefix", &self.cdn_prefix)
            //.field("encoding", &self.encoding)
            //.field("install", &self.install)
            .field("cache", &self.cache)
            .finish()
    }
}

impl CascClient {
    fn get_by_ckey(&self, ckey: Key) -> Result<Vec<u8>> {
        let ekey = self
            .encoding
            .c2e(ContentKey(ckey.0))
            .ok()
            .map(|k| Key(k.0))
            .context("Unknown ckey")?;
        self.get_by_ekey(ekey)
    }

    fn get_by_keys(&self, k: FileKeys) -> Result<Vec<u8>> {
        let ckey = k.ckey;
        let ekey_verify = self.encoding.c2e(ContentKey(ckey.0)).ok().map(|k| Key(k.0));
        if let Some(ekey_verify) = ekey_verify {
            ensure!(ekey_verify == k.ekey);
        }

        self.get_by_ekey(k.ekey)
    }

    fn get_by_ekey(&self, ekey: Key) -> Result<Vec<u8>> {
        let exe_file_path = format!(
            "{}data/{}",
            self.cdn_prefix,
            format_hex_key(&ekey.as_hex_string())
        );
        let bytes = self
            .cache
            .get(&exe_file_path, "data", &ekey.as_hex_string())?;
        let blted = blte::parse(ekey.0, &bytes)?;

        Ok(blted)
    }

    #[tracing::instrument(err)]
    fn get_client_binaries(&self) -> Result<()> {
        for exe in self
            .install
            .files
            .iter()
            .filter(|x| x.name.ends_with("Wow.exe"))
        {
            let ckey = exe.key;
            //let ekey = Key(self.encoding.c2e(ContentKey(ckey.0))?.0);

            tracing::debug!(
                exe_name = exe.name,
                ckey = ckey.as_hex_string(),
                "Downloading exe {} ckey {}",
                exe.name,
                ckey,
            );

            // let exe_file_path = format!(
            //     "{}data/{}",
            //     self.cdn_prefix,
            //     format_hex_key(&ekey.to_string())
            // );
            // let exe_data = self.cache.get(&exe_file_path, "data", &ekey.to_string())?;

            //let install_decompressed = blte::parse(ekey.0, &exe_data)?;
            let install_decompressed = self
                .get_by_ckey(ckey)
                .with_context(|| format!("get_by_ckey failed for {}", exe.name))?;
            let path = PathBuf::from(format!("root/{}", exe.name));
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, install_decompressed)?;

            use std::ops::Deref;
            tracing::info!(
                path = path.to_string_lossy().deref(),
                exe_name = exe.name,
                "Downloaded"
            );
        }

        Ok(())
        //dbg!(&cdns, &versions, cdn);
    }
}

#[tracing::instrument(err)]
fn cdn_casc_client(game: &str, region: &str) -> Result<CascClient> {
    let cdns = fetch(&format!("http://us.patch.battle.net:1119/{game}/cdns"))?;
    let versions = fetch(&format!("http://us.patch.battle.net:1119/{game}/versions"))?;

    let cdns = load_pipe_separated_vars(cdns.text()?);
    let versions = load_pipe_separated_vars(versions.text()?);

    let version_entry = versions
        .entries()
        .map(|x| x.collect::<Vec<_>>())
        .find(|x| x[0] == region)
        .context("no version found")?;

    tracing::debug!("{cdns:#?} {versions:#?}");

    let cdn = pick_cdn(&cdns);
    let (build_cfg_key, cdn_cfg_key) = (version_entry[1], version_entry[2]);

    tracing::info!(cdn = &cdn, "Picked CDN");

    let cdn_cfg = fetch(&format!("{cdn}config/{}", format_hex_key(cdn_cfg_key)))?.text()?;
    let build_cfg = fetch(&format!("{cdn}config/{}", format_hex_key(build_cfg_key)))?.text()?;

    let build_cfg_mini = &build_cfg[0..build_cfg.find("vfs-").unwrap_or(build_cfg.len())];

    tracing::debug!("{cdn_cfg} {build_cfg_mini}");

    let i = ini::Ini::load_from_str(&build_cfg)?;
    let sec = i.section(Option::<&str>::None).context("Invalid INI")?;

    let encoding = FileKeys::from_str(sec.get("encoding").context("Missing encoding")?)?;
    tracing::info!("Encoding keys: {encoding:?}");

    let cache = CacheByKey::new("cache");

    let encoding_file_path = format!(
        "{cdn}data/{}",
        format_hex_key(&encoding.ekey.as_hex_string())
    );
    let encoding_data = cache.get(&encoding_file_path, "data", &encoding.ekey.as_hex_string())?;

    let encoding_decompressed = blte::parse(encoding.ekey.0, &encoding_data)?;

    let encoding_parsed: encoding::Encoding = encoding::parse(&encoding_decompressed)?;
    tracing::info!("Parsed encoding. {}", encoding_parsed);

    let install = FileKeys::from_str(sec.get("install").context("Missing install")?)?;
    tracing::info!("Install keys: {install:?}");

    let encoding_install_key = encoding_parsed.c2e(ContentKey(install.ckey.0)).ok();
    if let Some(encoding_install_key) = encoding_install_key {
        tracing::info!("Verifying encoding and install ekey agree");
        ensure!(Key(encoding_install_key.0) == install.ekey);
    }

    let install_file_path = format!(
        "{cdn}data/{}",
        format_hex_key(&install.ekey.as_hex_string())
    );
    let install_data = cache.get(&install_file_path, "data", &install.ekey.as_hex_string())?;
    let install_decompressed = blte::parse(install.ekey.0, &install_data)?;
    let install = install::parse(&install_decompressed)?;

    Ok(CascClient {
        encoding: encoding_parsed,
        install,
        cache,
        cdn_prefix: cdn,
    })
}

static START_TIME: OnceLock<Instant> = OnceLock::new();

fn main() -> Result<()> {
    let timer: fn(&mut tracing_subscriber::fmt::format::Writer) -> Result<(), std::fmt::Error> =
        |x| {
            let duration = Instant::now() - *START_TIME.get_or_init(Instant::now);
            write!(x, "{:08.04}", duration.as_secs_f32())
        };
    tracing_subscriber::fmt()
        .compact()
        .with_timer(timer)
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let client = cdn_casc_client("wow", "us")?;
    client.get_client_binaries()?;

    Ok(())
}

use std::{
    collections::HashMap,
    convert::TryInto,
    fmt::Debug,
    io::Read,
    ops::Range,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use bytes::Buf;

#[derive(Debug)]
pub(crate) struct Index {
    pub(crate) map: HashMap<EncodingKey, (ArchiveKey, usize, usize)>,
}

pub mod encoding;

pub(crate) fn parse_index(name: ArchiveKey, data: &[u8]) -> Result<Index> {
    ensure!(data.len() >= 28, "truncated archive index data");
    let non_footer_size = data.len() - 28;
    let bytes_per_block = 4096 + 24;
    let num_blocks = non_footer_size / bytes_per_block;
    ensure!(
        non_footer_size % bytes_per_block == 0,
        "invalid archive index format"
    );
    let mut footer = &data[non_footer_size..];
    //ensure!(md5hash(footer) == name.0, "bad footer name");
    let toc_size = num_blocks * 24;
    let toc = &data[non_footer_size - toc_size..non_footer_size];
    ensure!(
        (md5hash(toc) >> 64) as u64 == footer.get_u64(),
        "archive index toc checksum"
    );
    ensure!(footer.get_u8() == 1, "unexpected archive index version");
    ensure!(
        footer.get_u8() == 0,
        "unexpected archive index nonzero byte"
    );
    ensure!(
        footer.get_u8() == 0,
        "unexpected archive index nonzero byte"
    );
    ensure!(footer.get_u8() == 4, "unexpected archive index block size");
    ensure!(
        footer.get_u8() == 4,
        "unexpected archive index offset bytes"
    );
    ensure!(footer.get_u8() == 4, "unexpected archive index size bytes");
    ensure!(footer.get_u8() == 16, "unexpected archive index key size");
    ensure!(
        footer.get_u8() == 8,
        "unexpected archive index checksum size"
    );
    let num_elements = footer.get_u32_le().try_into()?;
    let footer_checksum = footer.get_u64();
    assert!(!footer.has_remaining());
    {
        let mut footer_to_check = data[non_footer_size + 8..non_footer_size + 20].to_vec();
        footer_to_check.resize(20, 0);
        ensure!(
            (md5hash(&footer_to_check) >> 64) as u64 == footer_checksum,
            "archive index footer checksum"
        );
    };
    let mut map = HashMap::<EncodingKey, (ArchiveKey, usize, usize)>::new();
    let mut p = &data[..non_footer_size - toc_size];
    let mut entries = &toc[..(16 * num_blocks)];
    let mut blockhashes = &toc[(16 * num_blocks)..];
    for _ in 0..num_blocks {
        let mut block = &p[..4096];
        let block_checksum = blockhashes.get_u64();
        ensure!(
            (md5hash(block) >> 64) as u64 == block_checksum,
            "archive index block checksum"
        );
        let last_ekey = EncodingKey(entries.get_u128());
        let mut found = false;
        while block.remaining() >= 24 {
            let ekey = EncodingKey(block.get_u128());
            let size = block.get_u32().try_into()?;
            let offset = block.get_u32().try_into()?;
            ensure!(
                map.insert(ekey, (name, size, offset)).is_none(),
                "duplicate key in index"
            );
            if ekey == last_ekey {
                found = true;
                break;
            }
        }
        ensure!(found, "last ekey mismatch");
        p.advance(4096);
    }
    assert!(!p.has_remaining());
    assert!(!entries.has_remaining());
    assert!(!blockhashes.has_remaining());
    ensure!(map.len() == num_elements, "num_elements wrong in index");
    Ok(Index { map })
}
