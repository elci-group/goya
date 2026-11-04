//! goya-artifact — artifact identity, hashing, and signed manifests
//! (GY-ARTIFACT/1).
//!
//! An artifact gets three identities: an exact **byte hash** (FNV-1a 64),
//! a **perceptual hash** (8x8 block-mean average hash, stable under
//! recompression-style pixel perturbations), and a **watermark descriptor**
//! tying the artifact to a spectral watermark. Per spec §1, the byte hash is
//! computed LAST — after watermarking and encoding — so [`finalise`] is the
//! only public constructor path for an [`ArtifactIdentity`].
//!
//! Contract: [`ARTIFACT_CONTRACT`].
//!
//! Caveat on manifests: [`verify_signature`] only asserts *who signed* a
//! [`SignedManifest`] (signer membership in a trusted set). It does not and
//! cannot assert the truthfulness of the assertions inside it.

pub const ARTIFACT_CONTRACT: &str = "GY-ARTIFACT/1";

/// Cryptographic-strength primitive for watermark bindings, back-referenced
/// from the watermark crate's contract.
pub const WATERMARK_ALGORITHM: &str = "GY-DCT-MIDFREQ";

/// Descriptor of the spectral watermark embedded in (or expected for) an
/// artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatermarkDescriptor {
    /// Watermark algorithm identifier (e.g. `"GY-DCT-MIDFREQ"`).
    pub algorithm: &'static str,
    /// Algorithm version.
    pub version: u32,
    /// Identifier of the key used to embed the watermark.
    pub key_id: String,
    /// 64-bit payload carried by the watermark.
    pub payload_id: u64,
}

/// Structural media description of an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDescriptor {
    pub width: u32,
    pub height: u32,
    /// Pixel format tag (e.g. `"luma8"`).
    pub format: &'static str,
}

/// The three-part identity of an artifact. Construct only through
/// [`finalise`], which enforces spec §1's ordering invariant: the byte hash is
/// computed last, over the final (watermarked, encoded) bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactIdentity {
    /// Opaque artifact identifier assigned by the caller.
    pub artifact_id: String,
    /// FNV-1a 64 hash of the FINAL bytes (post-watermark, post-encode).
    pub byte_hash: u64,
    /// 8x8 block-mean perceptual hash of the final media, when media exists.
    pub perceptual_hash: Option<u64>,
    /// Reference into the provenance ledger for this artifact.
    pub provenance_ref: String,
    /// Watermark binding, when the artifact is watermarked.
    pub watermark: Option<WatermarkDescriptor>,
    /// Structural media description, when the artifact is media.
    pub media: MediaDescriptor,
}

/// A manifest binding an identity to a set of assertions, signed by a named
/// signer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedManifest {
    pub identity: ArtifactIdentity,
    pub assertions: Vec<String>,
    pub signer: String,
}

/// FNV-1a 64-bit hash, implemented locally (std-only, no dependencies).
pub fn content_hash(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// 8x8 block-mean average hash of a luma image (`pixels` row-major, `w` x
/// `h`).
///
/// The image is downsampled to 8x8 by averaging each block; each output bit is
/// the block mean compared against the overall mean, packed row-major into a
/// u64. Because bits depend on block *means*, small per-pixel perturbations
/// (recompression noise) do not flip bits, while structural changes do.
///
/// Blocks are the half-open ranges `[i*w/8, (i+1)*w/8)` x `[j*h/8,
/// (j+1)*h/8)`; for dimensions below 8 the ranges degenerate to nearest
/// sampling.
pub fn perceptual_hash(pixels: &[u8], width: u32, height: u32) -> u64 {
    let w = width as usize;
    let h = height as usize;
    debug_assert_eq!(pixels.len(), w * h, "pixel buffer must be w*h luma bytes");
    let mut block_means = [0.0f64; 64];
    for (bi, mean) in block_means.iter_mut().enumerate() {
        let bx = bi % 8;
        let by = bi / 8;
        let x0 = bx * w / 8;
        let x1 = ((bx + 1) * w / 8).max(x0 + 1);
        let y0 = by * h / 8;
        let y1 = ((by + 1) * h / 8).max(y0 + 1);
        let mut sum = 0u64;
        for y in y0..y1.min(h) {
            for x in x0..x1.min(w) {
                sum += u64::from(pixels[y * w + x]);
            }
        }
        let count = ((y1.min(h) - y0) * (x1.min(w) - x0)) as f64;
        *mean = sum as f64 / count.max(1.0);
    }
    let overall = block_means.iter().sum::<f64>() / 64.0;
    let mut hash = 0u64;
    for (i, m) in block_means.iter().enumerate() {
        if *m >= overall {
            hash |= 1u64 << i;
        }
    }
    hash
}

/// Build the final [`ArtifactIdentity`] for watermarked, encoded bytes.
///
/// This is the ONLY public constructor path for an [`ArtifactIdentity`] and it
/// enforces the spec §1 ordering invariant by signature: the caller must pass
/// the final bytes (post-watermark, post-encode), and the byte hash is
/// computed here — LAST — over exactly those bytes. Hashing anything other
/// than the final bytes would require calling a different function, so the
/// ordering cannot be accidentally inverted at a call site.
///
/// `perceptual_hash` is computed when the media is luma (`format == "luma8"`)
/// and the pixel buffer size matches `media.width * media.height`; otherwise
/// it is `None`.
pub fn finalise(
    artifact_id: &str,
    watermarked_bytes: &[u8],
    media: MediaDescriptor,
    provenance_ref: &str,
    watermark: Option<WatermarkDescriptor>,
) -> ArtifactIdentity {
    // Order per spec §1: watermark already applied by the caller -> encode ->
    // NOW hash the final bytes (last).
    let byte_hash = content_hash(watermarked_bytes);
    let perceptual_hash = if media.format == "luma8"
        && watermarked_bytes.len() == media.width as usize * media.height as usize
        && media.width > 0
        && media.height > 0
    {
        Some(perceptual_hash(
            watermarked_bytes,
            media.width,
            media.height,
        ))
    } else {
        None
    };
    ArtifactIdentity {
        artifact_id: artifact_id.to_string(),
        byte_hash,
        perceptual_hash,
        provenance_ref: provenance_ref.to_string(),
        watermark,
        media,
    }
}

/// Verify a manifest's signature. **Caveat:** this only asserts that the
/// manifest's `signer` is a member of `trusted_signers` — i.e. *who signed*.
/// It does NOT verify the truthfulness of the assertions, nor the integrity of
/// the identity against the artifact; those require out-of-band checks.
pub fn verify_signature(manifest: &SignedManifest, trusted_signers: &[&str]) -> bool {
    trusted_signers.iter().any(|s| *s == manifest.signer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(w: u32, h: u32) -> (Vec<u8>, u32, u32) {
        let pixels: Vec<u8> = (0..(w * h)).map(|i| ((i * 3) % 256) as u8).collect();
        (pixels, w, h)
    }

    fn media_of(w: u32, h: u32) -> MediaDescriptor {
        MediaDescriptor {
            width: w,
            height: h,
            format: "luma8",
        }
    }

    #[test]
    fn fnv1a_known_vectors() {
        assert_eq!(content_hash(b""), 0xcbf2_9ce4_8422_2325);
        // FNV-1a 64 of "a" is the well-known published vector.
        assert_eq!(content_hash(b"a"), 0xaf63_dc4c_8601_ec8c);
        // Different bytes -> different hash.
        assert_ne!(content_hash(b"abc"), content_hash(b"abd"));
    }

    #[test]
    fn perceptual_hash_robust_to_perturbation() {
        let (pixels, w, h) = luma(64, 64);
        let noisy: Vec<u8> = pixels
            .iter()
            .enumerate()
            .map(|(i, &p)| (p as i16 + if i % 3 == 0 { 2 } else { -1 }).clamp(0, 255) as u8)
            .collect();
        assert_eq!(
            perceptual_hash(&pixels, w, h),
            perceptual_hash(&noisy, w, h)
        );
    }

    #[test]
    fn perceptual_hash_changes_with_structure() {
        let (pixels, w, h) = luma(64, 64);
        let inverted: Vec<u8> = pixels.iter().map(|&p| 255 - p).collect();
        assert_ne!(
            perceptual_hash(&pixels, w, h),
            perceptual_hash(&inverted, w, h)
        );
    }

    #[test]
    fn finalise_hashes_last_over_final_bytes() {
        let (pixels, w, h) = luma(64, 64);
        let marked: Vec<u8> = pixels.iter().map(|&p| p.saturating_add(1)).collect();
        let wm = WatermarkDescriptor {
            algorithm: WATERMARK_ALGORITHM,
            version: 1,
            key_id: "k-2026-01".to_string(),
            payload_id: 0xAB_CD_EF_01,
        };
        let identity = finalise("art-1", &marked, media_of(w, h), "prov://art-1", Some(wm));
        // Byte hash must equal the hash of the FINAL (watermarked) bytes,
        // not the pre-watermark bytes.
        assert_eq!(identity.byte_hash, content_hash(&marked));
        assert_ne!(identity.byte_hash, content_hash(&pixels));
        assert_eq!(
            identity.perceptual_hash,
            Some(perceptual_hash(&marked, w, h))
        );
        assert_eq!(identity.artifact_id, "art-1");
        assert_eq!(identity.provenance_ref, "prov://art-1");
    }

    #[test]
    fn finalise_is_the_only_constructor_path() {
        // Byte hash inside an identity can only be produced by finalise;
        // simulate a caller trying to hash pre-watermark bytes: the resulting
        // identity cannot match the finalised one.
        let (pixels, w, h) = luma(64, 64);
        let marked: Vec<u8> = pixels.iter().map(|&p| p ^ 0x01).collect();
        let id_final = finalise("a", &marked, media_of(w, h), "p", None);
        let id_wrong = ArtifactIdentity {
            artifact_id: "a".to_string(),
            byte_hash: content_hash(&pixels), // hashed too early
            perceptual_hash: Some(perceptual_hash(&marked, w, h)),
            provenance_ref: "p".to_string(),
            watermark: None,
            media: media_of(w, h),
        };
        assert_ne!(id_final, id_wrong);
    }

    #[test]
    fn verify_signature_is_membership_only() {
        let (pixels, w, h) = luma(32, 32);
        let identity = finalise("a", &pixels, media_of(w, h), "p", None);
        let manifest = SignedManifest {
            identity,
            assertions: vec!["was_reviewed".to_string()],
            signer: "alice".to_string(),
        };
        assert!(verify_signature(&manifest, &["alice", "bob"]));
        assert!(!verify_signature(&manifest, &["bob"]));
        // Membership says nothing about truthfulness: an untrusted signer can
        // still author arbitrary assertions — verification must not be read
        // as endorsement.
        let forged = SignedManifest {
            assertions: vec!["completely_false_claim".to_string()],
            signer: "mallory".to_string(),
            identity: manifest.identity.clone(),
        };
        assert!(!verify_signature(&forged, &["alice"]));
    }

    #[test]
    fn perceptual_hash_handles_small_images() {
        let pixels = vec![10u8; 4 * 4];
        let hash = perceptual_hash(&pixels, 4, 4);
        // Uniform image: all means equal, all bits set.
        assert_eq!(hash, u64::MAX);
        // Buffer size mismatch must not panic; finalise degrades to None.
        let media = MediaDescriptor {
            width: 64,
            height: 64,
            format: "luma8",
        };
        let identity = finalise("small", &pixels, media, "p", None);
        assert_eq!(identity.perceptual_hash, None);
    }
}
