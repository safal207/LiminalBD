use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[derive(Debug, Clone)]
pub struct SeedParams {
    pub affinity: f32,
    pub base_metabolism: f32,
    pub core_pattern: String,
}

fn derive_seed_bytes(seed_str: &str) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    let seed_bytes = seed_str.as_bytes();
    if seed_bytes.is_empty() {
        return bytes;
    }
    for (i, slot) in bytes.iter_mut().enumerate() {
        let idx = i % seed_bytes.len();
        let value = seed_bytes[idx];
        *slot = value.wrapping_add((i * 37) as u8);
    }
    bytes
}

pub fn create_seed(seed_str: &str) -> SeedParams {
    let mut rng = ChaCha8Rng::from_seed(derive_seed_bytes(seed_str));
    let affinity = rng.gen::<f32>().clamp(0.0, 1.0);
    let base_metabolism = 0.3 + rng.gen::<f32>() * 0.7;
    let mut pattern = seed_str.trim().to_string();
    if pattern.is_empty() {
        pattern = format!("seed-{}", rng.gen::<u32>());
    }
    SeedParams {
        affinity,
        base_metabolism,
        core_pattern: pattern,
    }
}
