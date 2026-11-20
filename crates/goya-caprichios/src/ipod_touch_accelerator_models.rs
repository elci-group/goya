//! Per-model iPod touch accelerator caprichios.
//!
//! Every model below is its own caprichio: a dedicated module with contract
//! `goya.hardware.accelerator.ipod_touch.<model_slug>.v1`, stamped out by
//! [`crate::accelerator_models!`] from the shared engine. This file holds
//! only published catalog facts, in release order.

crate::accelerator_models!(
    "ipod_touch",
    (
        ipod_touch_1st_generation,
        "iPod touch (1st generation)",
        "Samsung S5L8900",
        None,
        None
    ),
    (
        ipod_touch_2nd_generation,
        "iPod touch (2nd generation)",
        "Samsung S5L8920",
        None,
        None
    ),
    (
        ipod_touch_3rd_generation,
        "iPod touch (3rd generation)",
        "Samsung S5L8922",
        None,
        None
    ),
    (
        ipod_touch_4th_generation,
        "iPod touch (4th generation)",
        "A4",
        None,
        None
    ),
    (
        ipod_touch_5th_generation,
        "iPod touch (5th generation)",
        "A5",
        Some(2u16),
        None
    ),
    (
        ipod_touch_6th_generation,
        "iPod touch (6th generation)",
        "A8",
        Some(4u16),
        Some(2u8)
    ),
    (
        ipod_touch_7th_generation,
        "iPod touch (7th generation)",
        "A10 Fusion",
        Some(6u16),
        Some(4u8)
    ),
);
