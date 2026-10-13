use ultrahdr::analyzer::analyze_image_potential;
use ultrahdr::engine::{
    assemble_ultra_hdr_jpeg, compute_hdr_and_gain_map, encode_gain_map_jpeg,
    encode_rgb_jpeg, generate_nits_heatmap, simulate_hdr_on_sdr, srgb_to_linear,
    TonalParams,
};

#[test]
fn test_gain_map_math() {
    let w = 50;
    let h = 50;
    let sdr_u8 = vec![128u8; (w * h * 3) as usize]; // ~0.21 linéaire

    let res = compute_hdr_and_gain_map(
        &sdr_u8,
        None,
        false,
        w,
        h,
        1000.0,
        203.0,
        1.0,
        1.0,
        0.4,
        TonalParams::default(),
        0.0,
    );

    assert_eq!(res.gain_log2.len(), (w * h) as usize);
    assert_eq!(res.hdr_nits.len(), (w * h * 3) as usize);
    assert!(res.gain_max >= 0.0);
}

#[test]
fn test_assemble_ultra_hdr_jpeg() {
    let w = 100;
    let h = 100;
    let mut sdr_u8 = vec![40u8; (w * h * 3) as usize];
    // Rendre la moitié droite lumineuse
    for y in 0..h {
        for x in 50..w {
            let idx = ((y * w + x) * 3) as usize;
            sdr_u8[idx] = 240;
            sdr_u8[idx + 1] = 240;
            sdr_u8[idx + 2] = 240;
        }
    }

    let res = compute_hdr_and_gain_map(
        &sdr_u8,
        None,
        false,
        w,
        h,
        1200.0,
        203.0,
        1.15,
        1.0,
        0.5,
        TonalParams::default(),
        0.0,
    );

    let base_jpeg = encode_rgb_jpeg(&sdr_u8, w, h, 94).expect("encodage base");
    let gm_jpeg = encode_gain_map_jpeg(&res.gain_log2, res.gain_min, res.gain_max, w, h, 88).expect("encodage gain map");

    let uhdr_bytes = assemble_ultra_hdr_jpeg(&base_jpeg, &gm_jpeg, res.gain_min, res.gain_max, 1200.0, 203.0, None);

    assert!(uhdr_bytes.len() > 1000);
    // Doit commencer par le marqueur SOI JPEG (0xFF 0xD8)
    assert_eq!(&uhdr_bytes[..2], &[0xff, 0xd8]);

    let text = String::from_utf8_lossy(&uhdr_bytes);
    // Doit contenir le namespace Adobe / Google Ultra HDR
    assert!(text.contains("http://ns.adobe.com/hdr-gain-map/1.0/"));
    assert!(text.contains("hdrgm:GainMapMax"));

    // Doit contenir le marqueur MPF CIPA DC-007
    let mpf_marker = b"MPF\0";
    assert!(uhdr_bytes.windows(4).any(|w| w == mpf_marker));
}

#[test]
fn test_sdr_simulation_and_heatmap() {
    let w = 60;
    let h = 60;
    let hdr_nits = vec![800.0f32; (w * h * 3) as usize];

    let sim_aces = simulate_hdr_on_sdr(&hdr_nits, w, h, 203.0, -1.0, "aces");
    assert_eq!(sim_aces.len(), (w * h * 3) as usize);

    let sim_reinhard = simulate_hdr_on_sdr(&hdr_nits, w, h, 203.0, 0.0, "reinhard");
    assert_eq!(sim_reinhard.len(), (w * h * 3) as usize);

    let heat = generate_nits_heatmap(&hdr_nits, w, h, 203.0, 1200.0);
    assert_eq!(heat.len(), (w * h * 3) as usize);
}

#[test]
fn test_analyzer_classification() {
    let w = 150;
    let h = 150;
    let mut linear_rgb = vec![0.02f32; (w * h * 3) as usize];
    // Haut de l'image chaud/lumineux (coucher de soleil)
    for y in 0..75 {
        for x in 0..w {
            let idx = ((y * w + x) * 3) as usize;
            linear_rgb[idx] = srgb_to_linear(1.0); // R fort
            linear_rgb[idx + 1] = srgb_to_linear(0.65); // G
            linear_rgb[idx + 2] = srgb_to_linear(0.15); // B faible
        }
    }

    let analysis = analyze_image_potential(false, &linear_rgb, 8.0, "JPEG");
    assert!(analysis.potential_score >= 70);
    assert!(["SUNSET", "LANDSCAPE", "NIGHT_NEON", "STUDIO"].contains(&analysis.category.as_str()));
    assert!(!analysis.recommended_preset.name.is_empty());
}

#[test]
fn test_contrast_modulation() {
    let w = 40;
    let h = 40;
    let mut sdr_u8 = vec![220u8; (w * h * 3) as usize];
    for i in 0..(w * h / 2) as usize {
        sdr_u8[i * 3] = 40;
        sdr_u8[i * 3 + 1] = 40;
        sdr_u8[i * 3 + 2] = 40;
    }

    let res_neutral = compute_hdr_and_gain_map(
        &sdr_u8, None, false, w, h, 1200.0, 203.0, 1.0, 1.0, 0.4,
        TonalParams::default(), 0.0,
    );

    let res_contrast = compute_hdr_and_gain_map(
        &sdr_u8, None, false, w, h, 1200.0, 203.0, 1.0, 1.0, 0.4,
        TonalParams { contrast: 0.5, ..Default::default() }, 0.0,
    );

    assert!(res_contrast.gain_max >= res_neutral.gain_max);
}

#[test]
fn test_vibrance_saturation() {
    let pixels = vec![
        120u8, 120u8, 120u8, // Gris neutre (sat = 0)
        200u8, 150u8, 120u8, // Teinte douce peu saturée
        255u8, 0u8, 0u8,     // Rouge pur ultra saturé (sat = 1.0)
    ];

    // Saturation positive augmente la chromaticité de la teinte douce
    let sat_boost = ultrahdr::engine::apply_vibrance_saturation(&pixels, 0.5, 0.0);
    assert_eq!(sat_boost[0], 120); // Neutre reste neutre
    assert!(sat_boost[3] >= pixels[3]); // R augmente
    assert!(sat_boost[5] <= pixels[5]); // B diminue (écart plus grand = plus saturé)

    // Vibrance positive booste la teinte douce sans saturer excessivement le rouge pur
    let vib_boost = ultrahdr::engine::apply_vibrance_saturation(&pixels, 0.0, 0.5);
    assert_eq!(vib_boost[0], 120); // Neutre inchangé
    assert!(vib_boost[3] >= pixels[3]); // Douce rehaussée
    assert_eq!(vib_boost[6], 255); // Rouge pur reste 255 (protégé)
}

#[test]
fn test_tone_curve() {
    let default_params = TonalParams::default();

    // 1. Sans ajustement, la courbe est strictement la diagonale y = x
    for k in 0..=10 {
        let x = k as f32 / 10.0;
        let y = ultrahdr::gui::eval_tone_curve(x, &default_params);
        assert!((y - x).abs() < 1e-4);
    }

    // 2. Avec contraste positif, les hautes lumières montent et les ombres descendent
    let contrast_params = TonalParams {
        contrast: 0.5,
        ..Default::default()
    };
    let y_high = ultrahdr::gui::eval_tone_curve(0.75, &contrast_params);
    let y_low = ultrahdr::gui::eval_tone_curve(0.25, &contrast_params);
    assert!(y_high > 0.75);
    assert!(y_low < 0.25);

    // 3. Toutes les sorties restent dans [0.0, 1.0]
    let extreme_params = TonalParams {
        contrast: 1.0,
        whites: 1.0,
        highlights: 1.0,
        shadows: 1.0,
        blacks: 1.0,
        ..Default::default()
    };
    for k in 0..=20 {
        let x = k as f32 / 20.0;
        let y = ultrahdr::gui::eval_tone_curve(x, &extreme_params);
        assert!((0.0..=1.0).contains(&y));
    }
}
