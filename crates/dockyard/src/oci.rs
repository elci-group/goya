//! Deterministic OCI image layout for one static executable.
//!
//! The layer is an uncompressed ustar archive. Digests are SHA-256 over the
//! exact bytes written. Created time is the Unix epoch.

use crate::{admit::Service, CONTENT_TYPE, CREATED, IMAGE_USER, METHOD, PROFILE, STATUS};
use sha2::{Digest, Sha256};

const UID: u32 = 65532;
const MODE: u32 = 0o555;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFiles {
    pub oci_layout: Vec<u8>,
    pub index_json: Vec<u8>,
    pub blobs: Vec<(String, Vec<u8>)>,
    pub executable_sha256: String,
    pub body_sha256: String,
    pub layer_sha256: String,
    pub config_sha256: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub name: String,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub is_dir: bool,
    pub data: Vec<u8>,
}

pub fn pack(binary: &[u8], service: &Service) -> Result<ImageFiles, &'static str> {
    if binary.is_empty() {
        return Err("executable is empty");
    }
    let layer = archive(&[
        entry("srv/", true, &[]),
        entry("srv/dockyard/", true, &[]),
        entry("srv/dockyard/service", false, binary),
    ])?;
    let layer_sha256 = sha256_hex(&layer);
    let executable_sha256 = sha256_hex(binary);
    let body_sha256 = sha256_hex(service.body.as_bytes());
    let config = config_json(service, &body_sha256, &layer_sha256);
    let config_sha256 = sha256_hex(config.as_bytes());
    let manifest = manifest_json(&config_sha256, config.len(), &layer_sha256, layer.len());
    let manifest_sha256 = sha256_hex(manifest.as_bytes());
    let index = index_json(&manifest_sha256, manifest.len());
    Ok(ImageFiles {
        oci_layout: b"{\"imageLayoutVersion\":\"1.0.0\"}".to_vec(),
        index_json: index.into_bytes(),
        blobs: vec![
            (layer_sha256.clone(), layer),
            (config_sha256.clone(), config.into_bytes()),
            (manifest_sha256.clone(), manifest.into_bytes()),
        ],
        executable_sha256,
        body_sha256,
        layer_sha256,
        config_sha256,
        manifest_sha256,
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_encode(Sha256::digest(bytes).as_slice())
}

fn entry<'a>(name: &'a str, is_dir: bool, data: &'a [u8]) -> ArchiveEntry {
    ArchiveEntry {
        name: name.to_string(),
        mode: MODE,
        uid: UID,
        gid: UID,
        is_dir,
        data: data.to_vec(),
    }
}

fn archive(entries: &[ArchiveEntry]) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::new();
    for item in entries {
        out.extend_from_slice(&header(item)?);
        if !item.is_dir {
            out.extend_from_slice(&item.data);
            let pad = (512 - (item.data.len() % 512)) % 512;
            out.extend(std::iter::repeat_n(0, pad));
        }
    }
    out.extend_from_slice(&[0u8; 1024]);
    Ok(out)
}

fn header(item: &ArchiveEntry) -> Result<[u8; 512], &'static str> {
    if item.name.len() > 100 || item.name.is_empty() || item.name.as_bytes().contains(&0) {
        return Err("tar name is not representable");
    }
    if item.is_dir && !item.data.is_empty() {
        return Err("directory payload is not empty");
    }
    let mut header = [0u8; 512];
    header[..item.name.len()].copy_from_slice(item.name.as_bytes());
    write_octal(&mut header[100..108], u64::from(item.mode))?;
    write_octal(&mut header[108..116], u64::from(item.uid))?;
    write_octal(&mut header[116..124], u64::from(item.gid))?;
    write_octal(&mut header[124..136], item.data.len() as u64)?;
    write_octal(&mut header[136..148], 0)?;
    header[148..156].fill(b' ');
    header[156] = if item.is_dir { b'5' } else { b'0' };
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header[265..272].copy_from_slice(b"nonroot");
    header[297..304].copy_from_slice(b"nonroot");
    let sum = checksum(&header);
    write_checksum(&mut header[148..156], sum)?;
    Ok(header)
}

fn write_octal(field: &mut [u8], value: u64) -> Result<(), &'static str> {
    let digits = field.len() - 1;
    let text = format!("{value:0digits$o}");
    if text.len() > digits {
        return Err("tar numeric field overflowed");
    }
    field[..digits].copy_from_slice(text.as_bytes());
    field[digits] = 0;
    Ok(())
}

fn write_checksum(field: &mut [u8], sum: u32) -> Result<(), &'static str> {
    let text = format!("{sum:06o}");
    if text.len() != 6 || field.len() != 8 {
        return Err("tar checksum field is the wrong width");
    }
    field[..6].copy_from_slice(text.as_bytes());
    field[6] = 0;
    field[7] = b' ';
    Ok(())
}

fn checksum(header: &[u8; 512]) -> u32 {
    header.iter().enumerate().fold(0u32, |sum, (index, byte)| {
        if (148..156).contains(&index) {
            sum + u32::from(b' ')
        } else {
            sum + u32::from(*byte)
        }
    })
}

pub fn read_archive(bytes: &[u8]) -> Result<Vec<ArchiveEntry>, &'static str> {
    if bytes.len() < 1024 || !bytes.len().is_multiple_of(512) {
        return Err("tar length is not a block multiple");
    }
    let mut entries = Vec::new();
    let mut offset = 0;
    while offset + 512 <= bytes.len() {
        let block: [u8; 512] = bytes[offset..offset + 512]
            .try_into()
            .map_err(|_| "tar block is short")?;
        offset += 512;
        if block == [0u8; 512] {
            if offset + 512 > bytes.len() || bytes[offset..offset + 512] != [0u8; 512] {
                return Err("tar trailer is incomplete");
            }
            if offset + 512 != bytes.len() {
                return Err("tar has bytes after the trailer");
            }
            return Ok(entries);
        }
        let stored = read_octal(&block[148..156])?;
        if stored != u64::from(checksum(&block)) {
            return Err("tar checksum does not match");
        }
        let name_end = block[..100]
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(100);
        let name = std::str::from_utf8(&block[..name_end]).map_err(|_| "tar name is not utf-8")?;
        if name.is_empty() || name.starts_with('/') || name.contains("..") {
            return Err("tar name is rejected");
        }
        let size = read_octal(&block[124..136])? as usize;
        let mode = read_octal(&block[100..108])? as u32;
        let uid = read_octal(&block[108..116])? as u32;
        let gid = read_octal(&block[116..124])? as u32;
        let is_dir = block[156] == b'5';
        if is_dir && size != 0 {
            return Err("directory entry has a payload");
        }
        if offset + size > bytes.len() {
            return Err("tar payload overruns the archive");
        }
        let data = bytes[offset..offset + size].to_vec();
        offset += size;
        let pad = (512 - (size % 512)) % 512;
        if offset + pad > bytes.len() || bytes[offset..offset + pad].iter().any(|byte| *byte != 0) {
            return Err("tar padding is not zero");
        }
        offset += pad;
        entries.push(ArchiveEntry {
            name: name.to_string(),
            mode,
            uid,
            gid,
            is_dir,
            data,
        });
    }
    Err("tar ended without a trailer")
}

fn read_octal(field: &[u8]) -> Result<u64, &'static str> {
    let text = field
        .iter()
        .take_while(|byte| **byte != 0 && **byte != b' ')
        .copied()
        .collect::<Vec<_>>();
    let text = std::str::from_utf8(&text).map_err(|_| "tar numeric field is not utf-8")?;
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("tar numeric field is not octal");
    }
    u64::from_str_radix(text, 8).map_err(|_| "tar numeric field overflowed")
}

fn config_json(service: &Service, body_sha256: &str, layer_diff: &str) -> String {
    format!(
        "{{\"architecture\":\"amd64\",\"config\":{{\"Entrypoint\":[\"/srv/dockyard/service\"],\"ExposedPorts\":{{\"{port}/tcp\":{{}}}},\"Labels\":{{\"org.elci.goya.dockyard.body-sha256\":\"{body_sha256}\",\"org.elci.goya.dockyard.content-type\":\"{CONTENT_TYPE}\",\"org.elci.goya.dockyard.method\":\"{METHOD}\",\"org.elci.goya.dockyard.path\":\"{path}\",\"org.elci.goya.dockyard.port\":\"{port}\",\"org.elci.goya.dockyard.profile\":\"{PROFILE}\",\"org.elci.goya.dockyard.qualification\":\"blocked\",\"org.elci.goya.dockyard.service\":\"{name}\",\"org.elci.goya.dockyard.status\":\"{STATUS}\"}},\"User\":\"{IMAGE_USER}\",\"WorkingDir\":\"/\"}},\"created\":\"{CREATED}\",\"history\":[{{\"created\":\"{CREATED}\",\"created_by\":\"dockyard {PROFILE}\"}}],\"os\":\"linux\",\"rootfs\":{{\"diff_ids\":[\"sha256:{layer_diff}\"],\"type\":\"layers\"}}}}",
        port = service.port,
        path = service.path,
        name = service.name,
    )
}

fn manifest_json(
    config_sha: &str,
    config_size: usize,
    layer_sha: &str,
    layer_size: usize,
) -> String {
    format!(
        "{{\"schemaVersion\":2,\"mediaType\":\"application/vnd.oci.image.manifest.v1+json\",\"config\":{{\"mediaType\":\"application/vnd.oci.image.config.v1+json\",\"digest\":\"sha256:{config_sha}\",\"size\":{config_size}}},\"layers\":[{{\"mediaType\":\"application/vnd.oci.image.layer.v1.tar\",\"digest\":\"sha256:{layer_sha}\",\"size\":{layer_size}}}]}}"
    )
}

fn index_json(manifest_sha: &str, manifest_size: usize) -> String {
    format!(
        "{{\"schemaVersion\":2,\"mediaType\":\"application/vnd.oci.image.index.v1+json\",\"manifests\":[{{\"mediaType\":\"application/vnd.oci.image.manifest.v1+json\",\"digest\":\"sha256:{manifest_sha}\",\"size\":{manifest_size},\"platform\":{{\"architecture\":\"amd64\",\"os\":\"linux\"}}}}]}}"
    )
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> Service {
        Service {
            name: "ping".to_string(),
            port: 39171,
            path: "/health".to_string(),
            body: "ok".to_string(),
        }
    }

    #[test]
    fn packing_the_same_bytes_is_identical() {
        let binary = b"not-an-elf-but-stable";
        let left = pack(binary, &service()).expect("pack");
        let right = pack(binary, &service()).expect("pack");
        assert_eq!(left, right);
        assert_eq!(left.oci_layout, b"{\"imageLayoutVersion\":\"1.0.0\"}");
        assert_eq!(left.layer_sha256, left.blobs[0].0);
        let config = &left.blobs[1].1;
        let text = std::str::from_utf8(config).expect("config");
        assert!(text.contains("\"User\":\"65532:65532\""));
        assert!(text.contains(CREATED));
        assert!(!text.contains("secret"));
        assert_eq!(left.blobs[0].0, sha256_hex(&left.blobs[0].1));
    }

    #[test]
    fn archive_roundtrip_keeps_one_executable() {
        let binary = b"abc";
        let image = pack(binary, &service()).expect("pack");
        let entries = read_archive(&image.blobs[0].1).expect("archive");
        assert_eq!(entries.len(), 3);
        assert!(entries[0].is_dir && entries[0].name == "srv/");
        assert!(entries[1].is_dir && entries[1].name == "srv/dockyard/");
        assert_eq!(entries[2].name, "srv/dockyard/service");
        assert_eq!(entries[2].data, binary);
        assert_eq!(entries[2].uid, 65532);
        assert_eq!(entries[2].mode, 0o555);
        assert!(entries.iter().all(|entry| entry.gid == 65532));
    }

    #[test]
    fn empty_executable_is_rejected() {
        assert!(pack(b"", &service()).is_err());
    }
}
