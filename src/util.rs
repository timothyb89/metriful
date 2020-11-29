/// Returns a f32 given an integral numerator and a u8 denominator, assumed to
/// be between 0-99 inclusive.
pub fn read_f32_with_u8_denom(int_part: impl Into<f32>, frac_part: u8) -> f32 {
  int_part.into() + (frac_part as f32 / 10f32)
}
