use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Identifier of a namespace within the cluster.
pub type NsId = String;

/// Role-based access control levels supported by the core.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Writer,
    Reader,
}

impl Role {
    /// Returns true if the role provides at least the capabilities of `required`.
    pub fn permits(self, required: Role) -> bool {
        matches!(
            (self, required),
            (Role::Admin, _)
                | (Role::Writer, Role::Writer | Role::Reader)
                | (Role::Reader, Role::Reader)
        )
    }
}

/// Metadata for an API key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKey {
    pub id: String,
    pub secret_hash: [u8; 32],
    pub role: Role,
    pub ns: NsId,
    pub created_ms: u64,
    pub disabled: bool,
}

impl ApiKey {
    pub fn new(
        id: impl Into<String>,
        secret_hash: [u8; 32],
        role: Role,
        ns: impl Into<String>,
        created_ms: u64,
    ) -> Self {
        ApiKey {
            id: id.into(),
            secret_hash,
            role,
            ns: ns.into(),
            created_ms,
            disabled: false,
        }
    }
}

/// Namespace quotas covering rate limits and resource bounds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Quotas {
    pub rps: u32,
    pub burst: u32,
    pub max_cells: u32,
    pub max_views: u32,
}

impl Default for Quotas {
    fn default() -> Self {
        Quotas {
            rps: 0,
            burst: 0,
            max_cells: 0,
            max_views: 0,
        }
    }
}

/// Combined access state for namespaces and API keys.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Access {
    pub keys: HashMap<String, ApiKey>,
    pub ns_quotas: HashMap<NsId, Quotas>,
}

impl Access {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_namespace(&mut self, ns: impl Into<String>) {
        let ns = ns.into();
        self.ns_quotas.entry(ns).or_insert_with(Quotas::default);
    }

    pub fn set_quota(&mut self, ns: impl Into<String>, quota: Quotas) {
        self.ns_quotas.insert(ns.into(), quota);
    }

    pub fn add_key(&mut self, key: ApiKey) -> Result<(), AccessError> {
        if self.keys.contains_key(&key.id) {
            return Err(AccessError::KeyExists(key.id));
        }
        if !self.ns_quotas.contains_key(&key.ns) {
            return Err(AccessError::UnknownNamespace(key.ns));
        }
        self.keys.insert(key.id.clone(), key);
        Ok(())
    }

    pub fn disable_key(&mut self, key_id: &str) -> Result<(), AccessError> {
        match self.keys.get_mut(key_id) {
            Some(key) => {
                key.disabled = true;
                Ok(())
            }
            None => Err(AccessError::UnknownKey(key_id.to_string())),
        }
    }

    pub fn authorize(&self, key_id: &str, required: Role, ns: &str) -> Result<Role, AccessError> {
        let key = self
            .keys
            .get(key_id)
            .ok_or_else(|| AccessError::UnknownKey(key_id.to_string()))?;
        if key.disabled {
            return Err(AccessError::KeyDisabled(key_id.to_string()));
        }
        if key.ns != ns {
            return Err(AccessError::WrongNamespace {
                key: key_id.to_string(),
                key_ns: key.ns.clone(),
                required_ns: ns.to_string(),
            });
        }
        if !key.role.permits(required) {
            return Err(AccessError::Forbidden {
                key: key_id.to_string(),
                role: key.role,
                required,
            });
        }
        Ok(key.role)
    }
}

/// Represents an access check failure.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AccessError {
    #[error("unknown key: {0}")]
    UnknownKey(String),
    #[error("key disabled: {0}")]
    KeyDisabled(String),
    #[error("key already exists: {0}")]
    KeyExists(String),
    #[error("namespace not registered: {0}")]
    UnknownNamespace(String),
    #[error("namespace mismatch for key {key}: key namespace is {key_ns}, required {required_ns}")]
    WrongNamespace {
        key: String,
        key_ns: String,
        required_ns: String,
    },
    #[error("forbidden: key {key} has role {role:?} but {required:?} is required")]
    Forbidden {
        key: String,
        role: Role,
        required: Role,
    },
}

/// Hashes an API secret using BLAKE3.
pub fn hash_secret(secret: impl AsRef<[u8]>) -> [u8; 32] {
    let hash = blake3::hash(secret.as_ref());
    *hash.as_bytes()
}

/// Simple token bucket for namespace rate limiting.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_per_ms: f64,
    last_refill_ms: u64,
}

impl TokenBucket {
    pub fn new(quotas: &Quotas, now_ms: u64) -> Self {
        let burst = quotas.burst.max(quotas.rps);
        let capacity = burst as f64;
        let refill_per_ms = if quotas.rps == 0 {
            0.0
        } else {
            quotas.rps as f64 / 1_000.0
        };
        TokenBucket {
            capacity,
            tokens: capacity,
            refill_per_ms,
            last_refill_ms: now_ms,
        }
    }

    fn refill(&mut self, now_ms: u64) {
        if now_ms <= self.last_refill_ms {
            return;
        }
        let elapsed = now_ms - self.last_refill_ms;
        self.last_refill_ms = now_ms;
        self.tokens = (self.tokens + self.refill_per_ms * elapsed as f64).min(self.capacity);
    }

    pub fn allow(&mut self, now_ms: u64, cost: f64) -> bool {
        self.refill(now_ms);
        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_bucket_respects_rps_and_burst() {
        let quotas = Quotas {
            rps: 3,
            burst: 5,
            ..Default::default()
        };
        let mut bucket = TokenBucket::new(&quotas, 0);
        for _ in 0..5 {
            assert!(bucket.allow(0, 1.0));
        }
        assert!(!bucket.allow(0, 1.0));
        assert!(!bucket.allow(100, 1.0));
        assert!(bucket.allow(334, 1.0));
        assert!(bucket.allow(668, 1.0));
        assert!(!bucket.allow(668, 1.0));
    }

    #[test]
    fn blake3_hash_is_stable() {
        let a = hash_secret("example-secret");
        let b = hash_secret("example-secret");
        assert_eq!(a, b);

        let c = hash_secret("another");
        assert_ne!(a, c);
    }

    #[test]
    fn authorization_checks_roles_and_namespaces() {
        let mut access = Access::new();
        access.register_namespace("alpha");
        access.register_namespace("beta");
        let key = ApiKey::new(
            "writer-alpha",
            hash_secret("secret"),
            Role::Writer,
            "alpha",
            0,
        );
        access.add_key(key).unwrap();

        assert_eq!(
            access
                .authorize("writer-alpha", Role::Reader, "alpha")
                .unwrap(),
            Role::Writer
        );
        assert_eq!(
            access
                .authorize("writer-alpha", Role::Writer, "alpha")
                .unwrap(),
            Role::Writer
        );
        assert!(matches!(
            access.authorize("writer-alpha", Role::Admin, "alpha"),
            Err(AccessError::Forbidden { .. })
        ));
        assert!(matches!(
            access.authorize("writer-alpha", Role::Reader, "beta"),
            Err(AccessError::WrongNamespace { .. })
        ));
    }
}
