//! Per-model iPhone accelerator caprichios.
//!
//! Every model below is its own caprichio: a dedicated module with contract
//! `goya.hardware.accelerator.iphone.<model_slug>.v1`, stamped out by
//! [`crate::accelerator_models!`] from the shared engine. This file holds
//! only published catalog facts, in release order.

crate::accelerator_models!(
    "iphone",
    (iphone, "iPhone", "Samsung S5L8900", None, None),
    (iphone_3g, "iPhone 3G", "Samsung S5L8920", None, None),
    (iphone_3gs, "iPhone 3GS", "Samsung S5L8930", None, None),
    (iphone_4, "iPhone 4", "A4", None, None),
    (iphone_4s, "iPhone 4s", "A5", Some(2u16), None),
    (iphone_5, "iPhone 5", "A6", Some(3u16), None),
    (iphone_5c, "iPhone 5c", "A6", Some(3u16), None),
    (iphone_5s, "iPhone 5s", "A7", Some(4u16), Some(1u8)),
    (iphone_6, "iPhone 6", "A8", Some(4u16), Some(2u8)),
    (iphone_6_plus, "iPhone 6 Plus", "A8", Some(4u16), Some(2u8)),
    (iphone_6s, "iPhone 6s", "A9", Some(6u16), Some(3u8)),
    (
        iphone_6s_plus,
        "iPhone 6s Plus",
        "A9",
        Some(6u16),
        Some(3u8)
    ),
    (
        iphone_se_1st_generation,
        "iPhone SE (1st generation)",
        "A9",
        Some(6u16),
        Some(3u8)
    ),
    (iphone_7, "iPhone 7", "A10 Fusion", Some(6u16), Some(4u8)),
    (
        iphone_7_plus,
        "iPhone 7 Plus",
        "A10 Fusion",
        Some(6u16),
        Some(4u8)
    ),
    (iphone_8, "iPhone 8", "A11 Bionic", Some(3u16), Some(5u8)),
    (
        iphone_8_plus,
        "iPhone 8 Plus",
        "A11 Bionic",
        Some(3u16),
        Some(5u8)
    ),
    (iphone_x, "iPhone X", "A11 Bionic", Some(3u16), Some(5u8)),
    (iphone_xr, "iPhone XR", "A12 Bionic", Some(4u16), Some(6u8)),
    (iphone_xs, "iPhone XS", "A12 Bionic", Some(4u16), Some(6u8)),
    (
        iphone_xs_max,
        "iPhone XS Max",
        "A12 Bionic",
        Some(4u16),
        Some(6u8)
    ),
    (iphone_11, "iPhone 11", "A13 Bionic", Some(4u16), Some(7u8)),
    (
        iphone_11_pro,
        "iPhone 11 Pro",
        "A13 Bionic",
        Some(4u16),
        Some(7u8)
    ),
    (
        iphone_11_pro_max,
        "iPhone 11 Pro Max",
        "A13 Bionic",
        Some(4u16),
        Some(7u8)
    ),
    (
        iphone_se_2nd_generation,
        "iPhone SE (2nd generation)",
        "A13 Bionic",
        Some(4u16),
        Some(7u8)
    ),
    (
        iphone_12_mini,
        "iPhone 12 mini",
        "A14 Bionic",
        Some(4u16),
        Some(8u8)
    ),
    (iphone_12, "iPhone 12", "A14 Bionic", Some(4u16), Some(8u8)),
    (
        iphone_12_pro,
        "iPhone 12 Pro",
        "A14 Bionic",
        Some(4u16),
        Some(8u8)
    ),
    (
        iphone_12_pro_max,
        "iPhone 12 Pro Max",
        "A14 Bionic",
        Some(4u16),
        Some(8u8)
    ),
    (
        iphone_13_mini,
        "iPhone 13 mini",
        "A15 Bionic",
        Some(4u16),
        Some(8u8)
    ),
    (iphone_13, "iPhone 13", "A15 Bionic", Some(4u16), Some(8u8)),
    (
        iphone_13_pro,
        "iPhone 13 Pro",
        "A15 Bionic",
        Some(5u16),
        Some(8u8)
    ),
    (
        iphone_13_pro_max,
        "iPhone 13 Pro Max",
        "A15 Bionic",
        Some(5u16),
        Some(8u8)
    ),
    (
        iphone_se_3rd_generation,
        "iPhone SE (3rd generation)",
        "A15 Bionic",
        Some(4u16),
        Some(8u8)
    ),
    (iphone_14, "iPhone 14", "A15 Bionic", Some(5u16), Some(8u8)),
    (
        iphone_14_plus,
        "iPhone 14 Plus",
        "A15 Bionic",
        Some(5u16),
        Some(8u8)
    ),
    (
        iphone_14_pro,
        "iPhone 14 Pro",
        "A16 Bionic",
        Some(5u16),
        Some(8u8)
    ),
    (
        iphone_14_pro_max,
        "iPhone 14 Pro Max",
        "A16 Bionic",
        Some(5u16),
        Some(8u8)
    ),
    (iphone_15, "iPhone 15", "A16 Bionic", Some(5u16), Some(8u8)),
    (
        iphone_15_plus,
        "iPhone 15 Plus",
        "A16 Bionic",
        Some(5u16),
        Some(8u8)
    ),
    (
        iphone_15_pro,
        "iPhone 15 Pro",
        "A17 Pro",
        Some(6u16),
        Some(9u8)
    ),
    (
        iphone_15_pro_max,
        "iPhone 15 Pro Max",
        "A17 Pro",
        Some(6u16),
        Some(9u8)
    ),
    (iphone_16, "iPhone 16", "A18", Some(5u16), Some(9u8)),
    (
        iphone_16_plus,
        "iPhone 16 Plus",
        "A18",
        Some(5u16),
        Some(9u8)
    ),
    (
        iphone_16_pro,
        "iPhone 16 Pro",
        "A18 Pro",
        Some(6u16),
        Some(9u8)
    ),
    (
        iphone_16_pro_max,
        "iPhone 16 Pro Max",
        "A18 Pro",
        Some(6u16),
        Some(9u8)
    ),
    (iphone_16e, "iPhone 16e", "A18", Some(4u16), Some(9u8)),
);
