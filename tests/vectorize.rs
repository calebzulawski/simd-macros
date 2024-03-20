#![feature(portable_simd)]

#[allow(unused)]
fn foo(x: core::simd::f32x4, y: core::simd::u32x4) -> core::simd::f32x4 {
    simd_macros::vectorize!(4, {
        if y == 1 {
            x
        } else {
            x + y as f32
        }
    })
}
