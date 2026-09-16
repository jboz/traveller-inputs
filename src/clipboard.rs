use sha2::{Digest, Sha256};

#[derive(Debug, Default)]
pub struct ClipboardGuard {
    pub last: Option<[u8; 32]>,
}

impl ClipboardGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Retourne true si le digest diffère du dernier connu (application légitime),
    /// false si identique (boucle : on n'applique pas).
    pub fn should_apply(&mut self, digest: [u8; 32]) -> bool {
        if self.last == Some(digest) {
            return false;
        }
        self.last = Some(digest);
        true
    }
}

pub fn digest_of(content: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(content.as_bytes());
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_stables_et_distincts() {
        assert_eq!(digest_of("abc"), digest_of("abc"));
        assert_ne!(digest_of("abc"), digest_of("abd"));
        assert_eq!(hex::encode(&digest_of("")[..4]), "e3b0c442");
    }

    #[test]
    fn double_applique_refusee() {
        let mut g = ClipboardGuard::new();
        let d = digest_of("texte");
        assert!(g.should_apply(d));
        assert!(!g.should_apply(d));
        assert!(g.should_apply(digest_of("autre")));
        assert!(g.should_apply(d));
    }
}