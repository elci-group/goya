//! Per-model Apple Watch accelerator caprichios.
//!
//! Every model below is its own caprichio: a dedicated module with contract
//! `goya.hardware.accelerator.apple_watch.<model_slug>.v1`, stamped out by
//! [`crate::accelerator_models!`] from the shared engine. This file holds
//! only published catalog facts, in release order.

crate::accelerator_models!(
    "apple_watch",
    (apple_watch_1st_generation, "Apple Watch (1st generation)", "S1", None, None),
    (apple_watch_series_1, "Apple Watch Series 1", "S1P", None, None),
    (apple_watch_series_2, "Apple Watch Series 2", "S2", None, None),
    (apple_watch_series_3, "Apple Watch Series 3", "S3", None, None),
    (apple_watch_series_4, "Apple Watch Series 4", "S4", None, None),
    (apple_watch_series_5, "Apple Watch Series 5", "S5", None, None),
    (apple_watch_se, "Apple Watch SE", "S5", None, None),
    (apple_watch_series_6, "Apple Watch Series 6", "S6", None, None),
    (apple_watch_series_7, "Apple Watch Series 7", "S7", None, None),
    (apple_watch_series_8, "Apple Watch Series 8", "S8", None, None),
    (apple_watch_ultra, "Apple Watch Ultra", "S8", None, None),
    (apple_watch_se_2nd_generation, "Apple Watch SE (2nd generation)", "S8", None, None),
    (apple_watch_series_9, "Apple Watch Series 9", "S9", None, None),
    (apple_watch_ultra_2, "Apple Watch Ultra 2", "S9", None, None),
    (apple_watch_series_10, "Apple Watch Series 10", "S10", None, None),
);
