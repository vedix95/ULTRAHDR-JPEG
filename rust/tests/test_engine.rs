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
