use rustdx_complete::{IncrementalIndicatorError, LegacyIndicatorError};

#[test]
fn explicit_names_preserve_both_original_public_types() {
    let incremental: rustdx_complete::indicators::IndicatorError =
        IncrementalIndicatorError::ZeroPeriod;
    assert!(matches!(
        rustdx_complete::indicators::try_ema(&[1.0], 0),
        Err(error) if error == incremental
    ));
    let legacy: rustdx_complete::IndicatorError =
        LegacyIndicatorError::insufficient_data("SMA".into(), 20, 1);
    assert!(matches!(
        rustdx_complete::Error::from(legacy),
        rustdx_complete::Error::Indicator(_)
    ));
}
