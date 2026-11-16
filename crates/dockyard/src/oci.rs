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
            if !entries
                .iter()
                .any(|entry: &ArchiveEntry| entry.name == "srv/dockyard/service")
            {
                return Err("tar executable is missing");
            }
            return Ok(entries);
        }
        let stored = read_octal(&block[148..156])?;
        if stored != u64::from(checksum(&block)) {
            return Err("tar checksum does not match");
        }
        if block[500..512].iter().any(|byte| *byte != 0) {
            return Err("tar header padding is not zero");
        }
        if block[148..154]
            .iter()
            .any(|byte| !(b'0'..=b'7').contains(byte))
            || block[154] != 0
            || block[155] != b' '
        {
            return Err("tar checksum field is not packed");
        }
        for field in [
            &block[100..108],
            &block[108..116],
            &block[116..124],
            &block[124..136],
            &block[136..148],
        ] {
            if !numeric_field_is_packed(field) {
                return Err("tar numeric field is not packed");
            }
        }
        let name_end = match block[..100].iter().position(|byte| *byte == 0) {
            Some(end) => end,
            None => return Err("tar name is not terminated"),
        };
        if block[name_end + 1..100].iter().any(|byte| *byte != 0) {
            return Err("tar name padding is not zero");
        }
        let name = std::str::from_utf8(&block[..name_end]).map_err(|_| "tar name is not utf-8")?;
        if name.is_empty() || name.starts_with('/') || name.contains("..") || name.contains("//") {
            return Err("tar name is rejected");
        }
        let size = read_octal(&block[124..136])? as usize;
        let mode = read_octal(&block[100..108])? as u32;
        let uid = read_octal(&block[108..116])? as u32;
        let gid = read_octal(&block[116..124])? as u32;
        let typeflag = block[156];
        let is_dir = typeflag == b'5';
        if is_dir && size != 0 {
            return Err("directory entry has a payload");
        }
        if !is_dir && typeflag != b'0' && typeflag != b'\0' {
            return Err("tar entry is not a regular file");
        }
        if (is_dir && !name.ends_with('/')) || (!is_dir && name.ends_with('/')) {
            return Err("tar name is rejected");
        }
        if &block[257..263] != b"ustar\0" {
            return Err("tar magic is not ustar");
        }
        if block[157..257].iter().any(|byte| *byte != 0) {
            return Err("tar link name is not empty");
        }
        if block[345..500].iter().any(|byte| *byte != 0) {
            return Err("tar prefix is not empty");
        }
        let mtime = read_octal(&block[136..148])?;
        if mtime != 0 {
            return Err("tar mtime is not zero");
        }
        if block[329..345].iter().any(|byte| *byte != 0) {
            return Err("tar device number is set");
        }
        if uid != UID || gid != UID {
            return Err("tar owner is not the profile user");
        }
        if mode != MODE {
            return Err("tar mode is not the profile mode");
        }
        if &block[263..265] != b"00" {
            return Err("tar version is not 00");
        }
        if &block[265..272] != b"nonroot" || block[272..297].iter().any(|byte| *byte != 0) {
            return Err("tar user name is not the profile name");
        }
        if &block[297..304] != b"nonroot" || block[304..329].iter().any(|byte| *byte != 0) {
            return Err("tar group name is not the profile name");
        }
        if name != "srv/" && name != "srv/dockyard/" && name != "srv/dockyard/service" {
            return Err("tar name is not in the profile");
        }
        if entries.iter().any(|entry| entry.name == name) {
            return Err("tar name is repeated");
        }
        let rank = match name {
            "srv/" => 0,
            "srv/dockyard/" => 1,
            "srv/dockyard/service" => 2,
            _ => return Err("tar name is not in the profile"),
        };
        if let Some(previous) = entries.last() {
            let previous_rank = match previous.name.as_str() {
                "srv/" => 0,
                "srv/dockyard/" => 1,
                "srv/dockyard/service" => 2,
                _ => return Err("tar name is not in the profile"),
            };
            if rank <= previous_rank {
                return Err("tar names are out of order");
            }
            if rank != previous_rank + 1 {
                return Err("tar names skip a profile entry");
            }
        }
        if !is_dir && size == 0 {
            return Err("tar executable is empty");
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

fn numeric_field_is_packed(field: &[u8]) -> bool {
    match field.split_last() {
        Some((0, digits)) => digits.iter().all(|byte| (b'0'..=b'7').contains(byte)),
        _ => false,
    }
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

    fn ustar_with_typeflag(payload: &[u8], typeflag: u8) -> Vec<u8> {
        let mut block = header(&entry("link", false, payload)).expect("header");
        block[156] = typeflag;
        let sum = checksum(&block);
        write_checksum(&mut block[148..156], sum).expect("checksum");
        let mut bytes = block.to_vec();
        bytes.extend_from_slice(payload);
        let pad = (512 - (payload.len() % 512)) % 512;
        bytes.extend(std::iter::repeat_n(0, pad));
        bytes.extend_from_slice(&[0u8; 1024]);
        bytes
    }

    #[test]
    fn special_typeflags_are_not_regular_files() {
        for typeflag in [b'2', b'1', b'x'] {
            for payload in [&[][..], &b"body"[..]] {
                let bytes = ustar_with_typeflag(payload, typeflag);
                assert_eq!(read_archive(&bytes), Err("tar entry is not a regular file"));
            }
        }
    }

    #[test]
    fn nul_typeflag_regular_file_roundtrips() {
        let payload = b"hi";
        let mut bytes = archive(&[entry("srv/dockyard/service", false, payload)]).expect("archive");
        assert_eq!(bytes[156], b'0');
        bytes[156] = b'\0';
        let mut block = [0u8; 512];
        block.copy_from_slice(&bytes[..512]);
        let sum = checksum(&block);
        write_checksum(&mut block[148..156], sum).expect("checksum");
        bytes[..512].copy_from_slice(&block);
        let entries = read_archive(&bytes).expect("read");
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].data, payload);
    }

    fn recompute_checksum(bytes: &mut [u8]) {
        let mut block = [0u8; 512];
        block.copy_from_slice(&bytes[..512]);
        let sum = checksum(&block);
        write_checksum(&mut block[148..156], sum).expect("checksum");
        bytes[..512].copy_from_slice(&block);
    }

    #[test]
    fn link_name_must_be_empty() {
        let mut bytes = archive(&[entry("file", false, b"hi")]).expect("archive");
        bytes[157] = b'L';
        recompute_checksum(&mut bytes);
        assert_eq!(read_archive(&bytes), Err("tar link name is not empty"));
    }

    #[test]
    fn prefix_must_be_empty() {
        let mut bytes = archive(&[entry("file", false, b"hi")]).expect("archive");
        bytes[345] = b'P';
        recompute_checksum(&mut bytes);
        assert_eq!(read_archive(&bytes), Err("tar prefix is not empty"));
    }

    #[test]
    fn magic_must_be_ustar() {
        let mut bytes = archive(&[entry("file", false, b"hi")]).expect("archive");
        bytes[257..263].copy_from_slice(b"ustar ");
        recompute_checksum(&mut bytes);
        assert_eq!(read_archive(&bytes), Err("tar magic is not ustar"));
    }

    #[test]
    fn mtime_must_be_zero() {
        let mut bytes = archive(&[entry("file", false, b"hi")]).expect("archive");
        write_octal(&mut bytes[136..148], 1).expect("mtime");
        recompute_checksum(&mut bytes);
        assert_eq!(read_archive(&bytes), Err("tar mtime is not zero"));
    }

    #[test]
    fn numeric_field_must_be_packed() {
        let mut block = header(&entry("file", false, b"hi")).expect("header");
        assert_eq!(block[147], 0);
        block[147] = b' ';
        let sum = checksum(&block);
        write_checksum(&mut block[148..156], sum).expect("checksum");
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar numeric field is not packed")
        );
    }

    #[test]
    fn checksum_field_must_be_packed() {
        let mut block = header(&entry("file", false, b"hi")).expect("header");
        block[155] = b'x';
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar checksum field is not packed")
        );
    }

    #[test]
    fn header_padding_must_be_zero() {
        let mut block = header(&entry("file", false, b"hi")).expect("header");
        block[500] = 1;
        recompute_checksum(&mut block);
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar header padding is not zero")
        );
    }

    #[test]
    fn device_number_must_be_unset() {
        let mut bytes = archive(&[entry("file", false, b"hi")]).expect("archive");
        bytes[329] = 1;
        recompute_checksum(&mut bytes);
        assert_eq!(read_archive(&bytes), Err("tar device number is set"));
    }

    fn archive_with_header(block: [u8; 512], payload: &[u8]) -> Vec<u8> {
        let mut bytes = block.to_vec();
        bytes.extend_from_slice(payload);
        let pad = (512 - (payload.len() % 512)) % 512;
        bytes.extend(std::iter::repeat_n(0, pad));
        bytes.extend_from_slice(&[0u8; 1024]);
        bytes
    }

    #[test]
    fn uid_change_is_not_the_profile_user() {
        let mut block = header(&entry("file", false, b"hi")).expect("header");
        write_octal(&mut block[108..116], u64::from(UID) + 1).expect("uid");
        let sum = checksum(&block);
        write_checksum(&mut block[148..156], sum).expect("checksum");
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar owner is not the profile user")
        );
    }

    #[test]
    fn mode_change_is_not_the_profile_mode() {
        let mut block = header(&entry("file", false, b"hi")).expect("header");
        write_octal(&mut block[100..108], u64::from(MODE) + 1).expect("mode");
        let sum = checksum(&block);
        write_checksum(&mut block[148..156], sum).expect("checksum");
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar mode is not the profile mode")
        );
    }

    #[test]
    fn version_change_is_not_00() {
        let mut block = header(&entry("file", false, b"hi")).expect("header");
        block[263] = b'1';
        let sum = checksum(&block);
        write_checksum(&mut block[148..156], sum).expect("checksum");
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar version is not 00")
        );
    }

    #[test]
    fn user_name_change_is_not_the_profile_name() {
        let mut block = header(&entry("file", false, b"hi")).expect("header");
        block[272] = b'x';
        let sum = checksum(&block);
        write_checksum(&mut block[148..156], sum).expect("checksum");
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar user name is not the profile name")
        );
    }

    #[test]
    fn group_name_change_is_not_the_profile_name() {
        let mut block = header(&entry("file", false, b"hi")).expect("header");
        block[304] = b'x';
        let sum = checksum(&block);
        write_checksum(&mut block[148..156], sum).expect("checksum");
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar group name is not the profile name")
        );
    }

    #[test]
    fn regular_file_name_must_not_end_with_slash() {
        let block = header(&entry("srv/dockyard/service/", false, b"hi")).expect("header");
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar name is rejected")
        );
    }

    #[test]
    fn directory_name_must_end_with_slash() {
        let block = header(&entry("srv", true, &[])).expect("header");
        assert_eq!(
            read_archive(&archive_with_header(block, &[])),
            Err("tar name is rejected")
        );
    }

    #[test]
    fn name_with_empty_segment_is_rejected() {
        let block = header(&entry("srv//dockyard", false, b"hi")).expect("header");
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar name is rejected")
        );
    }

    #[test]
    fn name_padding_must_be_zero() {
        let mut block = header(&entry("file", false, b"hi")).expect("header");
        let nul = block[..100]
            .iter()
            .position(|byte| *byte == 0)
            .expect("nul");
        block[nul + 1] = b' ';
        let sum = checksum(&block);
        write_checksum(&mut block[148..156], sum).expect("checksum");
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar name padding is not zero")
        );
    }

    #[test]
    fn name_must_be_nul_terminated() {
        let name = "a".repeat(100);
        let mut block = header(&entry(&name, false, b"hi")).expect("header");
        let sum = checksum(&block);
        write_checksum(&mut block[148..156], sum).expect("checksum");
        assert_eq!(
            read_archive(&archive_with_header(block, b"hi")),
            Err("tar name is not terminated")
        );
    }

    #[test]
    fn name_must_be_in_the_profile() {
        let bytes = archive(&[entry("file", false, b"hi")]).expect("archive");
        assert_eq!(read_archive(&bytes), Err("tar name is not in the profile"));
    }

    #[test]
    fn name_must_not_be_repeated() {
        let bytes =
            archive(&[entry("srv/", true, &[]), entry("srv/", true, &[])]).expect("archive");
        assert_eq!(read_archive(&bytes), Err("tar name is repeated"));
    }

    #[test]
    fn names_must_follow_profile_order() {
        let built = archive(&[entry("srv/dockyard/", true, &[]), entry("srv/", true, &[])]);
        assert_eq!(
            built.as_ref().map(|bytes| read_archive(bytes)),
            Ok(Err("tar names are out of order"))
        );
    }

    #[test]
    fn names_must_not_skip_a_profile_entry() {
        let built = archive(&[
            entry("srv/", true, &[]),
            entry("srv/dockyard/service", false, b"hi"),
        ]);
        assert_eq!(
            built.as_ref().map(|bytes| read_archive(bytes)),
            Ok(Err("tar names skip a profile entry"))
        );
        let built = archive(&[
            entry("srv/dockyard/", true, &[]),
            entry("srv/dockyard/service", false, b"hi"),
        ]);
        let entries = read_archive(&built.expect("archive")).expect("read");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn archive_must_contain_the_executable() {
        let built = archive(&[entry("srv/", true, &[]), entry("srv/dockyard/", true, &[])]);
        assert_eq!(
            built.as_ref().map(|bytes| read_archive(bytes)),
            Ok(Err("tar executable is missing"))
        );
    }

    #[test]
    fn executable_payload_must_not_be_empty() {
        let bytes = archive(&[entry("srv/dockyard/service", false, b"")]).expect("archive");
        assert_eq!(read_archive(&bytes), Err("tar executable is empty"));
    }

    #[test]
    fn config_json_omits_env_cmd_and_volumes() {
        let image = pack(b"not-an-elf-but-stable", &service()).expect("pack");
        let text = std::str::from_utf8(&image.blobs[1].1).expect("config");
        assert!(!text.contains("\"Env\""));
        assert!(!text.contains("\"Cmd\""));
        assert!(!text.contains("\"Volumes\""));
    }
}
