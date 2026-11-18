//! Per-model iPad accelerator caprichios.
//!
//! Every model below is its own caprichio: a dedicated module with contract
//! `goya.hardware.accelerator.ipad.<model_slug>.v1`, stamped out by
//! [`crate::accelerator_models!`] from the shared engine. This file holds
//! only published catalog facts, in release order.

// The shared macro is exported at the crate root.
pub use crate::accelerator_models;

crate::accelerator_models!(
    "ipad",
    (ipad, "iPad", "A4", None, None),
    (ipad_2, "iPad 2", "A5", Some(2u16), None),
    (ipad_3rd_generation, "iPad (3rd generation)", "A5X", Some(4u16), None),
    (ipad_4th_generation, "iPad (4th generation)", "A6X", Some(4u16), None),
    (ipad_air, "iPad Air", "A7", Some(4u16), Some(1u8)),
    (ipad_air_2, "iPad Air 2", "A8X", Some(8u16), Some(2u8)),
    (ipad_mini, "iPad mini", "A5", Some(2u16), None),
    (ipad_mini_2, "iPad mini 2", "A7", Some(4u16), Some(1u8)),
    (ipad_mini_3, "iPad mini 3", "A7", Some(4u16), Some(1u8)),
    (ipad_mini_4, "iPad mini 4", "A8X", Some(8u16), Some(2u8)),
    (ipad_pro_9_7_inch, "iPad Pro 9.7-inch", "A9X", Some(12u16), Some(3u8)),
    (ipad_pro_12_9_inch_1st_generation, "iPad Pro 12.9-inch (1st generation)", "A9X", Some(12u16), Some(3u8)),
    (ipad_pro_10_5_inch, "iPad Pro 10.5-inch", "A10X", Some(12u16), Some(4u8)),
    (ipad_pro_12_9_inch_2nd_generation, "iPad Pro 12.9-inch (2nd generation)", "A10X", Some(12u16), Some(4u8)),
    (ipad_5th_generation, "iPad (5th generation)", "A9", Some(6u16), Some(3u8)),
    (ipad_6th_generation, "iPad (6th generation)", "A10 Fusion", Some(6u16), Some(4u8)),
    (ipad_7th_generation, "iPad (7th generation)", "A10 Fusion", Some(6u16), Some(4u8)),
    (ipad_air_3rd_generation, "iPad Air (3rd generation)", "A12 Bionic", Some(4u16), Some(6u8)),
    (ipad_8th_generation, "iPad (8th generation)", "A12 Bionic", Some(4u16), Some(6u8)),
    (ipad_9th_generation, "iPad (9th generation)", "A13 Bionic", Some(4u16), Some(7u8)),
    (ipad_10th_generation, "iPad (10th generation)", "A14 Bionic", Some(4u16), Some(8u8)),
    (ipad_11th_generation, "iPad (11th generation)", "A16", Some(5u16), Some(8u8)),
    (ipad_air_4th_generation, "iPad Air (4th generation)", "A14 Bionic", Some(4u16), Some(8u8)),
    (ipad_air_5th_generation, "iPad Air (5th generation)", "M1", Some(8u16), Some(7u8)),
    (ipad_air_11_inch_m2, "iPad Air 11-inch (M2)", "M2", Some(9u16), Some(8u8)),
    (ipad_air_13_inch_m2, "iPad Air 13-inch (M2)", "M2", Some(9u16), Some(8u8)),
    (ipad_air_11_inch_m3, "iPad Air 11-inch (M3)", "M3", Some(9u16), Some(9u8)),
    (ipad_air_13_inch_m3, "iPad Air 13-inch (M3)", "M3", Some(9u16), Some(9u8)),
    (ipad_mini_5th_generation, "iPad mini (5th generation)", "A12 Bionic", Some(4u16), Some(6u8)),
    (ipad_mini_6th_generation, "iPad mini (6th generation)", "A15 Bionic", Some(5u16), Some(8u8)),
    (ipad_mini_7th_generation, "iPad mini (7th generation)", "A17 Pro", Some(6u16), Some(9u8)),
    (ipad_pro_11_inch_1st_generation, "iPad Pro 11-inch (1st generation)", "A12X Bionic", Some(7u16), Some(6u8)),
    (ipad_pro_12_9_inch_3rd_generation, "iPad Pro 12.9-inch (3rd generation)", "A12X Bionic", Some(7u16), Some(6u8)),
    (ipad_pro_11_inch_2nd_generation, "iPad Pro 11-inch (2nd generation)", "A12Z Bionic", Some(8u16), Some(6u8)),
    (ipad_pro_12_9_inch_4th_generation, "iPad Pro 12.9-inch (4th generation)", "A12Z Bionic", Some(8u16), Some(6u8)),
    (ipad_pro_11_inch_3rd_generation, "iPad Pro 11-inch (3rd generation)", "M1", Some(8u16), Some(7u8)),
    (ipad_pro_12_9_inch_5th_generation, "iPad Pro 12.9-inch (5th generation)", "M1", Some(8u16), Some(7u8)),
    (ipad_pro_11_inch_4th_generation, "iPad Pro 11-inch (4th generation)", "M2", Some(10u16), Some(8u8)),
    (ipad_pro_12_9_inch_6th_generation, "iPad Pro 12.9-inch (6th generation)", "M2", Some(10u16), Some(8u8)),
    (ipad_pro_11_inch_m4, "iPad Pro 11-inch (M4)", "M4", Some(9u16), Some(9u8)),
    (ipad_pro_13_inch_m4, "iPad Pro 13-inch (M4)", "M4", Some(10u16), Some(9u8)),
);
