# Embedding Security & Watermarking

## Overview

MarlOS applies security transforms to all vector embeddings before storing them. This protects your data in two ways:

1. **Rotation Matrix (Security)** - Makes extracted embeddings useless without the secret key
2. **Watermarking (Identification)** - Allows detection of embeddings that came from this tool

## How It Works

### Rotation Matrix

A **random orthogonal rotation matrix** is generated from a secret key using Gram-Schmidt orthogonalization. This matrix rotates all vectors in the embedding space.

```
Original Vector (1024D) → Rotation Matrix × Vector → Rotated Vector (1024D)
```

**Key Properties:**
- **Preserves cosine similarity** - Search still works perfectly
- **Preserves vector magnitudes** - Distances are unchanged
- **Deterministic** - Same key always produces same matrix
- **Reversible** - Original can be recovered with the key (inverse = transpose)

**Security Benefit:** If someone extracts the vectors from your database, they cannot:
- Use them with other embedding models
- Reverse-engineer the original text
- Compare them with vectors from other sources

### Watermarking

A subtle pattern is added to ~5% of vector dimensions. The pattern is:
- **Deterministic** - Based on a `watermark_id` (default: "MARLOS")
- **Detectable** - Correlation-based detection returns 0.7+ for watermarked vectors
- **Minimal impact** - 0.02 strength doesn't significantly affect similarity

```
Rotated Vector → Add Watermark Pattern → Final Vector
```

**Detection:** Call `detect_watermark(vector)` to get confidence score (0-1):
- `< 0.6` = Not watermarked (random correlation)
- `> 0.7` = Likely watermarked
- `> 0.9` = Definitely from this tool

## Configuration

Located in `TransformConfig`:

```rust
TransformConfig {
    secret_key: "marlos-default-key",  // Change for unique protection!
    enable_rotation: true,              // Security transform
    enable_watermark: true,             // Identification watermark
    watermark_strength: 0.02,           // Detection vs similarity tradeoff
    watermark_id: "MARLOS",             // Watermark identifier
}
```

## Implementation Files

- `src-tauri/src/embedding_transform.rs` - Core transform implementation
- `src-tauri/src/embeddings.rs` - Integration with EmbeddingManager

## Math Details

### Gram-Schmidt Orthogonalization

1. Generate `dim` random vectors from seeded PRNG
2. For each vector, subtract projections onto previous orthogonal vectors
3. Normalize to unit length
4. Result: orthogonal basis that forms rotation matrix

```rust
// Pseudocode
for v in random_vectors:
    u = v.clone()
    for prev in orthogonal_vectors:
        u -= project(u, prev)  // u - (u·prev / ||prev||²) * prev
    u = normalize(u)
    orthogonal_vectors.push(u)
```

### Watermark Pattern

1. Hash `watermark_id` to get seed
2. Select ~5% of dimensions deterministically
3. Set those dimensions to ±watermark_strength

```rust
// ~5% of dimensions get watermark
let num_dims = (total_dims * 0.05).max(10);
for i in 0..num_dims:
    dim_index = hash(seed, i) % total_dims
    sign = if (seed + i) % 2 == 0 { 1.0 } else { -1.0 }
    pattern[dim_index] = sign * watermark_strength
```

### Detection

Correlation-based detection:
```rust
correlation = dot(vector[watermark_dims], pattern[watermark_dims])
            / (||vector[watermark_dims]|| × ||pattern[watermark_dims]||)

confidence = (correlation + 1) / 2  // Map [-1, 1] to [0, 1]
```

## Security Considerations

1. **Change the default secret key** for production use
2. The rotation matrix is cached per dimension (first use computes, then reused)
3. Inverse transform is available if you need to recover original embeddings
4. Different keys produce completely different rotations (verified by tests)

## Tests

Run tests with:
```bash
cargo test embedding_transform --features tauri-app --lib
```

Tests verify:
- Rotation preserves cosine similarity
- Watermark is detectable after transform
- Inverse transform recovers original
- Different keys produce different results
