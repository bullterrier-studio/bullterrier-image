// The CPU's seeded random numbers (`ops/random.rs`: SplitMix64 and its `mix`, `unit`), bit for bit, on the 32-bit words WGSL
// has. A 64-bit word is a vec2<u32>: x the low half, y the high half. A kernel that draws a number for a pixel calls
// `mix3` with the CPU's own seed and words, then `unit53` or `standard_normal`. Nothing here keeps state.

fn u64_add(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let low = a.x + b.x;
    let carry = select(0u, 1u, low < a.x);
    return vec2<u32>(low, a.y + b.y + carry);
}

fn u64_xor(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    return a ^ b;
}

// a >> n for 0 < n < 32
fn u64_shr(a: vec2<u32>, n: u32) -> vec2<u32> {
    return vec2<u32>((a.x >> n) | (a.y << (32u - n)), a.y >> n);
}

// the 64-bit product of two 32-bit numbers, from 16-bit halves
fn mul_32(a: u32, b: u32) -> vec2<u32> {
    let a0 = a & 0xffffu;
    let a1 = a >> 16u;
    let b0 = b & 0xffffu;
    let b1 = b >> 16u;
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    let mid = p01 + (p00 >> 16u);
    let mid2 = (mid & 0xffffu) + p10;
    return vec2<u32>((mid2 << 16u) | (p00 & 0xffffu), p11 + (mid >> 16u) + (mid2 >> 16u));
}

// a * b modulo 2^64
fn u64_mul(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let low = mul_32(a.x, b.x);
    return vec2<u32>(low.x, low.y + a.x * b.y + a.y * b.x);
}

// the word of SplitMix64 after `state` is advanced (`splitmix` in ops/random.rs); the advanced state is `state + GOLDEN`
fn splitmix_word(advanced: vec2<u32>) -> vec2<u32> {
    var word = advanced;
    word = u64_mul(u64_xor(word, u64_shr(word, 30u)), vec2<u32>(0x1ce4e5b9u, 0xbf58476du));
    word = u64_mul(u64_xor(word, u64_shr(word, 27u)), vec2<u32>(0x133111ebu, 0x94d049bbu));
    return u64_xor(word, u64_shr(word, 31u));
}

const GOLDEN: vec2<u32> = vec2<u32>(0x7f4a7c15u, 0x9e3779b9u);

// `mix(seed, &[a, b, c])` of ops/random.rs, each word below 2^32
fn mix3(seed: u32, a: u32, b: u32, c: u32) -> vec2<u32> {
    var state = splitmix_word(u64_add(vec2<u32>(seed, 0u), GOLDEN));
    state = splitmix_word(u64_add(u64_xor(state, vec2<u32>(a, 0u)), GOLDEN));
    state = splitmix_word(u64_add(u64_xor(state, vec2<u32>(b, 0u)), GOLDEN));
    return splitmix_word(u64_add(u64_xor(state, vec2<u32>(c, 0u)), GOLDEN));
}

// `mix(seed, &[a, b, c, d])`, each word below 2^32
fn mix4(seed: u32, a: u32, b: u32, c: u32, d: u32) -> vec2<u32> {
    var state = splitmix_word(u64_add(vec2<u32>(seed, 0u), GOLDEN));
    state = splitmix_word(u64_add(u64_xor(state, vec2<u32>(a, 0u)), GOLDEN));
    state = splitmix_word(u64_add(u64_xor(state, vec2<u32>(b, 0u)), GOLDEN));
    state = splitmix_word(u64_add(u64_xor(state, vec2<u32>(c, 0u)), GOLDEN));
    return splitmix_word(u64_add(u64_xor(state, vec2<u32>(d, 0u)), GOLDEN));
}

// `mix(seed, &[a])` and `mix(seed, &[a, b])`
fn mix1(seed: u32, a: u32) -> vec2<u32> {
    let state = splitmix_word(u64_add(vec2<u32>(seed, 0u), GOLDEN));
    return splitmix_word(u64_add(u64_xor(state, vec2<u32>(a, 0u)), GOLDEN));
}

fn mix2(seed: u32, a: u32, b: u32) -> vec2<u32> {
    let state = splitmix_word(u64_add(u64_xor(splitmix_word(u64_add(vec2<u32>(seed, 0u), GOLDEN)), vec2<u32>(a, 0u)), GOLDEN));
    return splitmix_word(u64_add(u64_xor(state, vec2<u32>(b, 0u)), GOLDEN));
}

// `unit(word)`: the top 53 bits over 2^53, in 0 to 1 (to f32's precision)
fn unit53(word: vec2<u32>) -> f32 {
    let top = word.y >> 11u;
    let low = (word.x >> 11u) | (word.y << 21u);
    return (f32(top) * 4294967296.0 + f32(low)) * 1.1102230246251565e-16;
}

// `1 - unit(word)`, taken from the integers so a value near 1 keeps its precision (the CPU works it in f64)
fn one_minus_unit53(word: vec2<u32>) -> f32 {
    let top = word.y >> 11u;
    let low = (word.x >> 11u) | (word.y << 21u);
    let left_low = 0u - low;
    let left_top = 0x200000u - top - select(0u, 1u, low != 0u);
    return (f32(left_top) * 4294967296.0 + f32(left_low)) * 1.1102230246251565e-16;
}

// `standard_normal(seed, stream, at)` of ops/noise.rs: Box-Muller from the draws of parts 0 and 1
fn standard_normal(seed: u32, stream: u32, at: u32) -> f32 {
    let radius = sqrt(-2.0 * log(one_minus_unit53(mix3(seed, stream, at, 0u))));
    return radius * cos(6.283185307179586 * unit53(mix3(seed, stream, at, 1u)));
}
