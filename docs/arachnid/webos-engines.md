# webOS TV web engines — quick lookup

Generated from `arachnid models` on 20 November 2026. The data is
`goya_caprichios::webos_engine_caprichio` (`goya.platform.webos.engine.v1`)
and `goya_caprichios::webos_tv_models` (one caprichio per series,
`goya.platform.webos.<series>.v1`). To regenerate, run `arachnid models`
and `arachnid platforms`. Do not edit the tables by hand.

To look up a single TV:

```
arachnid lookup model:OLED65C3PUA      # model number (sticker or Settings › General › About This TV)
arachnid lookup reported:9.2.0         # version shown in Settings › … › TV Information
arachnid lookup 'ua:Mozilla/5.0 (Web0S; …) Chrome/108.0.5359.211 …'
```

## Platform → engine

| Platform | Model year | TV reports | Engine | Back with no history | Gamepad passthrough |
| --- | --- | --- | --- | --- | --- |
| webOS TV 4.0 | 2018 | 4.0 – 4.4 | Chromium 53 (53.0.2785.34) | Home | no |
| webOS TV 4.5 | 2019 | 4.5 – 4.10 | Chromium 53 (53.0.2785.34) | Home | no |
| webOS TV 5.0 | 2020 | 5.x | Chromium 68 (68.0.3440.106) | Home | no |
| webOS TV 6.0 | 2021 | 6.x | Chromium 79 (79.0.3945.79) | exit popup | no |
| webOS TV 22 | 2022 | 7.x | Chromium 87 (87.0.4280.88) | exit popup | no |
| webOS TV 23 | 2023 | 8.x | Chromium 94 (94.0.4606.128) | exit popup | yes |
| webOS TV 24 | 2024 | 9.x | Chromium 108 (108.0.5359.211) | exit popup | yes |
| webOS TV 25 | 2025 | 10.x | Chromium 120 (120.0.6099.270) | exit popup | yes |
| webOS TV 26 | 2026 | 11.x | Chromium 132 (132.0.6834.207) | exit popup | yes |

Below the floor, and refused by every Arachnid lookup:
* webOS 1.0 (2014, WebKit 537.41) and 2.0 (2015, WebKit 538.2).
* webOS 3.0/3.5 (2016–17, Chromium 38).

From webOS 22 on, the version the TV reports is the marketing name minus
15. For example, webOS 24 reports 9.x.

## Feature gates

The first Chromium release that shipped each feature. A target platform
supports a feature when its engine is at least that version.

| Feature | Chromium | First platform |
| --- | --- | --- |
| `KeyboardEvent.key`, IntersectionObserver | 51 | 4.0 |
| async/await, Pointer Events | 55 | 5.0 |
| CSS Grid | 57 | 5.0 |
| Object rest/spread | 60 | 5.0 |
| ES modules | 61 | 5.0 |
| ResizeObserver | 64 | 5.0 |
| Optional chaining, nullish coalescing | 80 | 22 |
| Flex `gap` | 84 | 22 |
| `:focus-visible` | 86 | 22 |
| `aspect-ratio` | 88 | 23 |

Arachnid's generated code needs none of these. It is ES5 with
absolutely positioned CSS, so one build runs on every platform from 4.0
on.

## Model years and series

*Shipped* is the release the TV left the factory with. It is the oldest
software the TV can be running, so it is what Arachnid targets. *Ceiling*
is the newest release the TV can be upgraded to:
* through LG's Re:New schedule (January 2025): 2021 StanbyME; 2022 OLED,
  8K QNED and StanbyME Go; all of 2023 and 2024;
* or where a TV of that series has been seen running a later release (C3,
  G3 and G5 on webOS 26).

A dash means no upgrade is known.

Series that are not listed still resolve by model year, because LG ships
one release per year across its whole range.

| Year | Series | Code | Shipped | Chromium | Ceiling | Caprichio |
| --- | --- | --- | --- | --- | --- | --- |
| 2018 | OLED B8 | `B8` | 4.0 | 53 | - | `goya.platform.webos.oled_b8.v1` |
| 2018 | OLED C8 | `C8` | 4.0 | 53 | - | `goya.platform.webos.oled_c8.v1` |
| 2018 | OLED E8 | `E8` | 4.0 | 53 | - | `goya.platform.webos.oled_e8.v1` |
| 2018 | OLED W8 | `W8` | 4.0 | 53 | - | `goya.platform.webos.oled_w8.v1` |
| 2018 | NanoCell SK8000 / SK8070 | `SK80` | 4.0 | 53 | - | `goya.platform.webos.nanocell_sk80.v1` |
| 2018 | NanoCell SK9000 | `SK90` | 4.0 | 53 | - | `goya.platform.webos.nanocell_sk90.v1` |
| 2018 | NanoCell SK9500 | `SK95` | 4.0 | 53 | - | `goya.platform.webos.nanocell_sk95.v1` |
| 2018 | UHD UK6300 | `UK63` | 4.0 | 53 | - | `goya.platform.webos.uhd_uk63.v1` |
| 2018 | UHD UK6500 / UK6570 | `UK65` | 4.0 | 53 | - | `goya.platform.webos.uhd_uk65.v1` |
| 2018 | UHD UK7700 | `UK77` | 4.0 | 53 | - | `goya.platform.webos.uhd_uk77.v1` |
| 2019 | OLED B9 | `B9` | 4.5 | 53 | - | `goya.platform.webos.oled_b9.v1` |
| 2019 | OLED C9 | `C9` | 4.5 | 53 | - | `goya.platform.webos.oled_c9.v1` |
| 2019 | OLED E9 | `E9` | 4.5 | 53 | - | `goya.platform.webos.oled_e9.v1` |
| 2019 | OLED W9 | `W9` | 4.5 | 53 | - | `goya.platform.webos.oled_w9.v1` |
| 2019 | OLED Z9 | `Z9` | 4.5 | 53 | - | `goya.platform.webos.oled_z9.v1` |
| 2019 | NanoCell SM81 | `SM81` | 4.5 | 53 | - | `goya.platform.webos.nanocell_sm81.v1` |
| 2019 | NanoCell SM86 | `SM86` | 4.5 | 53 | - | `goya.platform.webos.nanocell_sm86.v1` |
| 2019 | NanoCell SM90 | `SM90` | 4.5 | 53 | - | `goya.platform.webos.nanocell_sm90.v1` |
| 2019 | NanoCell SM95 | `SM95` | 4.5 | 53 | - | `goya.platform.webos.nanocell_sm95.v1` |
| 2019 | NanoCell SM99 | `SM99` | 4.5 | 53 | - | `goya.platform.webos.nanocell_sm99.v1` |
| 2019 | UHD UM6970 | `UM69` | 4.5 | 53 | - | `goya.platform.webos.uhd_um69.v1` |
| 2019 | UHD UM71 | `UM71` | 4.5 | 53 | - | `goya.platform.webos.uhd_um71.v1` |
| 2019 | UHD UM73 | `UM73` | 4.5 | 53 | - | `goya.platform.webos.uhd_um73.v1` |
| 2019 | UHD UM74 | `UM74` | 4.5 | 53 | - | `goya.platform.webos.uhd_um74.v1` |
| 2019 | UHD UM75 | `UM75` | 4.5 | 53 | - | `goya.platform.webos.uhd_um75.v1` |
| 2019 | UHD UM80 | `UM80` | 4.5 | 53 | - | `goya.platform.webos.uhd_um80.v1` |
| 2019 | FHD LM57 | `LM57` | 4.5 | 53 | - | `goya.platform.webos.fhd_lm57.v1` |
| 2020 | OLED BX | `BX` | 5 | 68 | - | `goya.platform.webos.oled_bx.v1` |
| 2020 | OLED CX | `CX` | 5 | 68 | - | `goya.platform.webos.oled_cx.v1` |
| 2020 | OLED GX | `GX` | 5 | 68 | - | `goya.platform.webos.oled_gx.v1` |
| 2020 | OLED WX | `WX` | 5 | 68 | - | `goya.platform.webos.oled_wx.v1` |
| 2020 | OLED ZX | `ZX` | 5 | 68 | - | `goya.platform.webos.oled_zx.v1` |
| 2020 | NanoCell NANO80 (2020) | `NANO80` | 5 | 68 | - | `goya.platform.webos.nano80_2020.v1` |
| 2020 | NanoCell NANO81 (2020) | `NANO81` | 5 | 68 | - | `goya.platform.webos.nano81_2020.v1` |
| 2020 | NanoCell NANO85 (2020) | `NANO85` | 5 | 68 | - | `goya.platform.webos.nano85_2020.v1` |
| 2020 | NanoCell NANO86 (2020) | `NANO86` | 5 | 68 | - | `goya.platform.webos.nano86_2020.v1` |
| 2020 | NanoCell NANO90 (2020) | `NANO90` | 5 | 68 | - | `goya.platform.webos.nano90_2020.v1` |
| 2020 | NanoCell NANO91 (2020) | `NANO91` | 5 | 68 | - | `goya.platform.webos.nano91_2020.v1` |
| 2020 | NanoCell NANO95 (2020) | `NANO95` | 5 | 68 | - | `goya.platform.webos.nano95_2020.v1` |
| 2020 | NanoCell NANO99 (2020) | `NANO99` | 5 | 68 | - | `goya.platform.webos.nano99_2020.v1` |
| 2020 | UHD UN70 | `UN70` | 5 | 68 | - | `goya.platform.webos.uhd_un70.v1` |
| 2020 | UHD UN71 | `UN71` | 5 | 68 | - | `goya.platform.webos.uhd_un71.v1` |
| 2020 | UHD UN8xxx | `UN8` | 5 | 68 | - | `goya.platform.webos.uhd_un8.v1` |
| 2020 | UHD UN9xxx | `UN9` | 5 | 68 | - | `goya.platform.webos.uhd_un9.v1` |
| 2021 | OLED A1 | `A1` | 6 | 79 | - | `goya.platform.webos.oled_a1.v1` |
| 2021 | OLED B1 | `B1` | 6 | 79 | - | `goya.platform.webos.oled_b1.v1` |
| 2021 | OLED C1 | `C1` | 6 | 79 | - | `goya.platform.webos.oled_c1.v1` |
| 2021 | OLED G1 | `G1` | 6 | 79 | - | `goya.platform.webos.oled_g1.v1` |
| 2021 | OLED Z1 | `Z1` | 6 | 79 | - | `goya.platform.webos.oled_z1.v1` |
| 2021 | OLED R1 | `R1` | 6 | 79 | - | `goya.platform.webos.oled_r1.v1` |
| 2021 | QNED90 (2021) | `QNED90` | 6 | 79 | - | `goya.platform.webos.qned90_2021.v1` |
| 2021 | QNED91 (2021) | `QNED91` | 6 | 79 | - | `goya.platform.webos.qned91_2021.v1` |
| 2021 | QNED95 (2021) | `QNED95` | 6 | 79 | - | `goya.platform.webos.qned95_2021.v1` |
| 2021 | QNED97 (2021) | `QNED97` | 6 | 79 | - | `goya.platform.webos.qned97_2021.v1` |
| 2021 | QNED99 (2021) | `QNED99` | 6 | 79 | - | `goya.platform.webos.qned99_2021.v1` |
| 2021 | NanoCell NANO75 (2021) | `NANO75` | 6 | 79 | - | `goya.platform.webos.nano75_2021.v1` |
| 2021 | NanoCell NANO80 (2021) | `NANO80` | 6 | 79 | - | `goya.platform.webos.nano80_2021.v1` |
| 2021 | NanoCell NANO81 (2021) | `NANO81` | 6 | 79 | - | `goya.platform.webos.nano81_2021.v1` |
| 2021 | NanoCell NANO85 (2021) | `NANO85` | 6 | 79 | - | `goya.platform.webos.nano85_2021.v1` |
| 2021 | NanoCell NANO86 (2021) | `NANO86` | 6 | 79 | - | `goya.platform.webos.nano86_2021.v1` |
| 2021 | NanoCell NANO88 (2021) | `NANO88` | 6 | 79 | - | `goya.platform.webos.nano88_2021.v1` |
| 2021 | NanoCell NANO90 (2021) | `NANO90` | 6 | 79 | - | `goya.platform.webos.nano90_2021.v1` |
| 2021 | NanoCell NANO91 (2021) | `NANO91` | 6 | 79 | - | `goya.platform.webos.nano91_2021.v1` |
| 2021 | NanoCell NANO96 (2021) | `NANO96` | 6 | 79 | - | `goya.platform.webos.nano96_2021.v1` |
| 2021 | NanoCell NANO99 (2021) | `NANO99` | 6 | 79 | - | `goya.platform.webos.nano99_2021.v1` |
| 2021 | UHD UP70 | `UP70` | 6 | 79 | - | `goya.platform.webos.uhd_up70.v1` |
| 2021 | UHD UP71 | `UP71` | 6 | 79 | - | `goya.platform.webos.uhd_up71.v1` |
| 2021 | UHD UP75 | `UP75` | 6 | 79 | - | `goya.platform.webos.uhd_up75.v1` |
| 2021 | UHD UP76 | `UP76` | 6 | 79 | - | `goya.platform.webos.uhd_up76.v1` |
| 2021 | UHD UP8xxx | `UP8` | 6 | 79 | - | `goya.platform.webos.uhd_up8.v1` |
| 2021 | UHD UP80 | `UP80` | 6 | 79 | - | `goya.platform.webos.uhd_up80.v1` |
| 2021 | UHD UP81 | `UP81` | 6 | 79 | - | `goya.platform.webos.uhd_up81.v1` |
| 2021 | UHD UP9xxx | `UP9` | 6 | 79 | - | `goya.platform.webos.uhd_up9.v1` |
| 2021 | StanbyME 27ART10 | `ART10` | 6 | 79 | 25 | `goya.platform.webos.stanbyme_art10.v1` |
| 2022 | OLED A2 | `A2` | 22 | 87 | 25 | `goya.platform.webos.oled_a2.v1` |
| 2022 | OLED B2 | `B2` | 22 | 87 | 25 | `goya.platform.webos.oled_b2.v1` |
| 2022 | OLED C2 | `C2` | 22 | 87 | 25 | `goya.platform.webos.oled_c2.v1` |
| 2022 | OLED G2 | `G2` | 22 | 87 | 25 | `goya.platform.webos.oled_g2.v1` |
| 2022 | OLED Z2 | `Z2` | 22 | 87 | 25 | `goya.platform.webos.oled_z2.v1` |
| 2022 | OLED R2 | `R2` | 22 | 87 | 25 | `goya.platform.webos.oled_r2.v1` |
| 2022 | OLED CS | `CS` | 22 | 87 | 25 | `goya.platform.webos.oled_cs.v1` |
| 2022 | QNED80 (2022) | `QNED80` | 22 | 87 | - | `goya.platform.webos.qned80_2022.v1` |
| 2022 | QNED82 (2022) | `QNED82` | 22 | 87 | - | `goya.platform.webos.qned82_2022.v1` |
| 2022 | QNED85 (2022) | `QNED85` | 22 | 87 | - | `goya.platform.webos.qned85_2022.v1` |
| 2022 | QNED90 (2022) | `QNED90` | 22 | 87 | - | `goya.platform.webos.qned90_2022.v1` |
| 2022 | QNED91 (2022) | `QNED91` | 22 | 87 | - | `goya.platform.webos.qned91_2022.v1` |
| 2022 | QNED95 8K (2022) | `QNED95` | 22 | 87 | 25 | `goya.platform.webos.qned95_2022.v1` |
| 2022 | QNED97 (2022) | `QNED97` | 22 | 87 | - | `goya.platform.webos.qned97_2022.v1` |
| 2022 | QNED99 8K (2022) | `QNED99` | 22 | 87 | 25 | `goya.platform.webos.qned99_2022.v1` |
| 2022 | NanoCell NANO75 (2022) | `NANO75` | 22 | 87 | - | `goya.platform.webos.nano75_2022.v1` |
| 2022 | NanoCell NANO80 (2022) | `NANO80` | 22 | 87 | - | `goya.platform.webos.nano80_2022.v1` |
| 2022 | NanoCell NANO81 (2022) | `NANO81` | 22 | 87 | - | `goya.platform.webos.nano81_2022.v1` |
| 2022 | NanoCell NANO85 (2022) | `NANO85` | 22 | 87 | - | `goya.platform.webos.nano85_2022.v1` |
| 2022 | NanoCell NANO86 (2022) | `NANO86` | 22 | 87 | - | `goya.platform.webos.nano86_2022.v1` |
| 2022 | NanoCell NANO88 (2022) | `NANO88` | 22 | 87 | - | `goya.platform.webos.nano88_2022.v1` |
| 2022 | NanoCell NANO90 (2022) | `NANO90` | 22 | 87 | - | `goya.platform.webos.nano90_2022.v1` |
| 2022 | NanoCell NANO91 (2022) | `NANO91` | 22 | 87 | - | `goya.platform.webos.nano91_2022.v1` |
| 2022 | NanoCell NANO96 (2022) | `NANO96` | 22 | 87 | - | `goya.platform.webos.nano96_2022.v1` |
| 2022 | NanoCell NANO99 (2022) | `NANO99` | 22 | 87 | - | `goya.platform.webos.nano99_2022.v1` |
| 2022 | UHD UQ70 | `UQ70` | 22 | 87 | - | `goya.platform.webos.uhd_uq70.v1` |
| 2022 | UHD UQ71 | `UQ71` | 22 | 87 | - | `goya.platform.webos.uhd_uq71.v1` |
| 2022 | UHD UQ75 | `UQ75` | 22 | 87 | - | `goya.platform.webos.uhd_uq75.v1` |
| 2022 | UHD UQ8xxx | `UQ8` | 22 | 87 | - | `goya.platform.webos.uhd_uq8.v1` |
| 2022 | UHD UQ9xxx | `UQ9` | 22 | 87 | - | `goya.platform.webos.uhd_uq9.v1` |
| 2022 | OLED Flex 42LX3 | `LX3` | 22 | 87 | 25 | `goya.platform.webos.flex_lx3.v1` |
| 2022 | StanbyME Go 27LX5 | `LX5` | 22 | 87 | 25 | `goya.platform.webos.stanbyme_go_lx5.v1` |
| 2023 | OLED A3 | `A3` | 23 | 94 | 25 | `goya.platform.webos.oled_a3.v1` |
| 2023 | OLED B3 | `B3` | 23 | 94 | 25 | `goya.platform.webos.oled_b3.v1` |
| 2023 | OLED C3 | `C3` | 23 | 94 | 26 | `goya.platform.webos.oled_c3.v1` |
| 2023 | OLED G3 | `G3` | 23 | 94 | 26 | `goya.platform.webos.oled_g3.v1` |
| 2023 | OLED M3 | `M3` | 23 | 94 | 25 | `goya.platform.webos.oled_m3.v1` |
| 2023 | OLED Z3 | `Z3` | 23 | 94 | 25 | `goya.platform.webos.oled_z3.v1` |
| 2023 | OLED R3 | `R3` | 23 | 94 | 25 | `goya.platform.webos.oled_r3.v1` |
| 2023 | QNED8x (2023) | `QNED8` | 23 | 94 | 25 | `goya.platform.webos.qned8_2023.v1` |
| 2023 | QNED9x (2023) | `QNED9` | 23 | 94 | 25 | `goya.platform.webos.qned9_2023.v1` |
| 2023 | UHD UR7xxx | `UR7` | 23 | 94 | 25 | `goya.platform.webos.uhd_ur7.v1` |
| 2023 | UHD UR8xxx | `UR8` | 23 | 94 | 25 | `goya.platform.webos.uhd_ur8.v1` |
| 2023 | UHD UR9xxx | `UR9` | 23 | 94 | 25 | `goya.platform.webos.uhd_ur9.v1` |
| 2024 | OLED B4 | `B4` | 24 | 108 | 25 | `goya.platform.webos.oled_b4.v1` |
| 2024 | OLED C4 | `C4` | 24 | 108 | 25 | `goya.platform.webos.oled_c4.v1` |
| 2024 | OLED G4 | `G4` | 24 | 108 | 25 | `goya.platform.webos.oled_g4.v1` |
| 2024 | OLED M4 | `M4` | 24 | 108 | 25 | `goya.platform.webos.oled_m4.v1` |
| 2024 | QNED (2024) | `QNED` | 24 | 108 | 25 | `goya.platform.webos.qned_2024.v1` |
| 2024 | NanoCell (2024) | `NANO` | 24 | 108 | 25 | `goya.platform.webos.nano_2024.v1` |
| 2024 | UHD UT7xxx | `UT7` | 24 | 108 | 25 | `goya.platform.webos.uhd_ut7.v1` |
| 2024 | UHD UT8xxx | `UT8` | 24 | 108 | 25 | `goya.platform.webos.uhd_ut8.v1` |
| 2024 | UHD UT9xxx | `UT9` | 24 | 108 | 25 | `goya.platform.webos.uhd_ut9.v1` |
| 2025 | OLED B5 | `B5` | 25 | 120 | - | `goya.platform.webos.oled_b5.v1` |
| 2025 | OLED C5 | `C5` | 25 | 120 | - | `goya.platform.webos.oled_c5.v1` |
| 2025 | OLED G5 | `G5` | 25 | 120 | 26 | `goya.platform.webos.oled_g5.v1` |
| 2025 | OLED M5 | `M5` | 25 | 120 | - | `goya.platform.webos.oled_m5.v1` |
| 2025 | QNED (2025) | `QNED` | 25 | 120 | - | `goya.platform.webos.qned_2025.v1` |
| 2025 | UHD UA7xxx | `UA7` | 25 | 120 | - | `goya.platform.webos.uhd_ua7.v1` |
| 2025 | UHD UA8xxx | `UA8` | 25 | 120 | - | `goya.platform.webos.uhd_ua8.v1` |
| 2026 | OLED B6 | `B6` | 26 | 132 | - | `goya.platform.webos.oled_b6.v1` |
| 2026 | OLED C6 | `C6` | 26 | 132 | - | `goya.platform.webos.oled_c6.v1` |
| 2026 | OLED G6 | `G6` | 26 | 132 | - | `goya.platform.webos.oled_g6.v1` |
| 2026 | OLED W6 | `W6` | 26 | 132 | - | `goya.platform.webos.oled_w6.v1` |
| 2026 | QNED (2026) | `QNED` | 26 | 132 | - | `goya.platform.webos.qned_2026.v1` |
| 2026 | UHD NU80 | `NU80` | 26 | 132 | - | `goya.platform.webos.uhd_nu80.v1` |

## Reading a model number

| Pattern | Example | Year from |
| --- | --- | --- |
| `OLED` + size + series letter + year | `OLED65C3PUA` | `8`=2018, `9`=2019, `X`=2020, `1`–`5`=2021–2025 |
| `OLED…CS…` | `OLED65CSPSA` | 2022 |
| size + `U`/`S`/`L` + year letter + digits | `55UQ7570PUJ`, `65SM9000PUA`, `43LM5760PTC` | `K` 2018, `M` 2019, `N` 2020, `P` 2021, `Q` 2022, `R` 2023, `T` 2024, `A` 2025 |
| size + `NANO`/`QNED`/`NU` + digits + year letter | `65QNED85TUA`, `43NU800BPSC`, `55QNED826QB` | `N`…`A` as above, `B` 2026; US 2020–23 codes put `U` first (`80URA`) |
| size + `LX` + digit + year letter | `42LX3Q6LA` | as above |
| `27ART10…` | StanbyME | 2021 |

Refused, never guessed:
* An `OLED…6…` model number. The digit `6` is both 2016 (webOS 3.0) and
  2026 (webOS 26). Use `webos:` or `reported:` instead.
* Model years before 2018.
* Anything that does not match these patterns.

## Sources

* LG, "Web API and Web Engine": engine and user agent per platform.
* LG, "appinfo.json" and "Magic Remote": app metadata and key codes.
* tab-tv.com, "LG TV operating system by year": model year to platform,
  and series lists.
* Glasshouse tested-TV list: versions reported by 47 real TVs, the
  marketing-name mapping, and observed upgrades.
* TechRadar / FlatpanelsHD, January 2025: LG Re:New upgrade schedule.
* LG 2026 press releases: G6, C6, B6 and W6 run webOS 26.

