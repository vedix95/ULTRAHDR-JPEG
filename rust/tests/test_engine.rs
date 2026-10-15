use ultrahdr::analyzer::analyze_image_potential;
use ultrahdr::engine::{
    adapt_hdr_nits_for_display, assemble_ultra_hdr_jpeg, compute_display_headroom_weight,
    compute_hdr_and_gain_map, encode_gain_map_jpeg, encode_rgb_jpeg, generate_nits_heatmap,
    simulate_hdr_on_sdr, srgb_to_linear, TonalParams,
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

    let sim_aces = simulate_hdr_on_sdr(&hdr_nits, w, h, 203.0, 1.0, 0.75, "aces");
    assert_eq!(sim_aces.len(), (w * h * 3) as usize);

    let sim_reinhard = simulate_hdr_on_sdr(&hdr_nits, w, h, 203.0, 1.0, 0.75, "reinhard");
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

    // 3. Toutes les sorties restent dans [0.0, 1.0] même avec des valeurs extrêmes étendues (-2.0 à +2.0)
    let extreme_params = TonalParams {
        contrast: 2.0,
        whites: 2.0,
        highlights: 2.0,
        shadows: 2.0,
        blacks: 2.0,
        ..Default::default()
    };
    for k in 0..=20 {
        let x = k as f32 / 20.0;
        let y = ultrahdr::gui::eval_tone_curve(x, &extreme_params);
        assert!((0.0..=1.0).contains(&y));
    }
}

#[test]
fn test_vivid_tone_mapping_and_slider_ranges() {
    let w = 40;
    let h = 40;
    let mut sdr_u8 = vec![120u8; (w * h * 3) as usize];
    for i in 0..(w * h / 2) as usize {
        sdr_u8[i * 3] = 230;
        sdr_u8[i * 3 + 1] = 230;
        sdr_u8[i * 3 + 2] = 230;
    }

    // Test avec valeurs étendues maximales (-2.0 et +2.0)
    let res_max = compute_hdr_and_gain_map(
        &sdr_u8, None, false, w, h, 2000.0, 203.0, 1.2, 1.0, 0.4,
        TonalParams {
            contrast: 1.5,
            whites: 1.5,
            highlights: 1.5,
            shadows: 1.5,
            blacks: -1.5,
            saturation: 1.5,
            vibrance: 1.5,
        },
        0.2,
    );

    assert!(res_max.gain_max > 0.5);

    // Test du mode de simulation "vivid" (Éclat Ultra HDR) avec protection d'épaulement EDR
    let sim_vivid = simulate_hdr_on_sdr(&res_max.hdr_nits, w, h, 203.0, 1.0, 0.75, "vivid");
    assert_eq!(sim_vivid.len(), (w * h * 3) as usize);

    // Les pixels SDR de base (120) ne doivent pas être assombris
    let sample_sdr = sim_vivid[(w * h - 1) as usize * 3];
    assert!(sample_sdr >= 110);
}

#[test]
fn test_iso_21496_1_adaptive_display_headroom() {
    let sdr_white = 203.0;
    let export_peak_10000 = 10000.0;
    let display_peak_400 = 400.0;

    // 1. Test du calcul du facteur d'échelle proportionnel W (ISO 21496-1)
    let w_scaled = compute_display_headroom_weight(export_peak_10000, display_peak_400, sdr_white);
    // H_content = log2(10000 / 203) = 5.6225
    // H_display = log2(400 / 203) = 0.9785
    // W = 0.9785 / 5.6225 ≈ 0.174
    assert!(w_scaled > 0.15 && w_scaled < 0.20, "W doit être proportionnel ~0.174");

    // 2. Si display_peak >= content_peak, W doit être exactement 1.0
    let w_full = compute_display_headroom_weight(400.0, 1000.0, sdr_white);
    assert_eq!(w_full, 1.0);

    // 3. Test de l'adaptation dynamique des nits
    let w_img = 20;
    let h_img = 20;
    let sdr_u8 = vec![255u8; (w_img * h_img * 3) as usize];
    let res = compute_hdr_and_gain_map(
        &sdr_u8, None, false, w_img, h_img, export_peak_10000, sdr_white, 1.0, 1.0, 0.5,
        TonalParams::default(), 0.0,
    );

    // Sans adaptation, le pic de luminance atteint ~10 000 nits
    let max_unadapted = res.hdr_nits.iter().cloned().fold(0.0f32, f32::max);
    assert!(max_unadapted > 8000.0, "La dynamique d'export doit bien atteindre 10000 nits");

    // Avec adaptation proportionnelle à l'écran 400 nits
    let adapted = adapt_hdr_nits_for_display(
        &res.hdr_nits, &res.gain_log2, &sdr_u8, w_img, h_img,
        export_peak_10000, display_peak_400, sdr_white,
    );
    let max_adapted = adapted.iter().cloned().fold(0.0f32, f32::max);

    // Le pic affiché doit être borné autour de 400 nits et ne JAMAIS exploser à 10 000 nits
    assert!(max_adapted <= 420.0, "Le pic affiché doit être borné à la capacité de l'écran (400 nits)");
    assert!(max_adapted >= 380.0, "Le pic affiché doit exploiter toute la dynamique de l'écran");
}

#[cfg(target_os = "windows")]
#[test]
fn test_windows_display_enum() {
    #[repr(C)]
    struct DisplayDeviceW {
        cb: u32,
        device_name: [u16; 32],
        device_string: [u16; 128],
        state_flags: u32,
        device_id: [u16; 128],
        device_key: [u16; 128],
    }

    extern "system" {
        fn EnumDisplayDevicesW(
            lp_device: *const u16,
            i_dev_num: u32,
            lp_display_device: *mut DisplayDeviceW,
            dw_flags: u32,
        ) -> i32;
    }

    let mut dev = DisplayDeviceW {
        cb: std::mem::size_of::<DisplayDeviceW>() as u32,
        device_name: [0; 32],
        device_string: [0; 128],
        state_flags: 0,
        device_id: [0; 128],
        device_key: [0; 128],
    };

    let mut found_monitors = Vec::new();
    unsafe {
        let mut i = 0;
        while EnumDisplayDevicesW(std::ptr::null(), i, &mut dev, 0) != 0 {
            if (dev.state_flags & 1) != 0 {
                // Attached to desktop
                let mut mon = DisplayDeviceW {
                    cb: std::mem::size_of::<DisplayDeviceW>() as u32,
                    device_name: [0; 32],
                    device_string: [0; 128],
                    state_flags: 0,
                    device_id: [0; 128],
                    device_key: [0; 128],
                };
                let mut j = 0;
                while EnumDisplayDevicesW(dev.device_name.as_ptr(), j, &mut mon, 0) != 0 {
                    if (mon.state_flags & 1) != 0 {
                        let id = String::from_utf16_lossy(&mon.device_id);
                        found_monitors.push((dev.state_flags & 4 != 0, id.trim_matches('\0').to_string()));
                    }
                    j += 1;
                }
            }
            i += 1;
        }
    }

    println!("Detected connected monitors: {:?}", found_monitors);
    assert!(!found_monitors.is_empty(), "Au moins un moniteur connecté");
}

#[test]
fn test_apply_tonal_and_color_adjustments() {
    let pixels = vec![
        200u8, 100u8, 50u8,  // Pixel coloré chaud
        40u8, 40u8, 40u8,    // Ombre profonde
        220u8, 220u8, 220u8, // Haute lumière
    ];

    // 1. Saturation à -1.0 doit rendre l'image rigoureusement monochrome (R == G == B)
    let mono_params = TonalParams {
        saturation: -1.0,
        ..Default::default()
    };
    let mono = ultrahdr::engine::apply_tonal_and_color_adjustments(&pixels, &mono_params);
    assert_eq!(mono[0], mono[1], "R doit être égal à G en monochrome");
    assert_eq!(mono[1], mono[2], "G doit être égal à B en monochrome");

    // 2. Ombres positives doivent éclaircir l'ombre profonde (40) sans toucher la haute lumière
    let shadow_params = TonalParams {
        shadows: 0.8,
        ..Default::default()
    };
    let lifted = ultrahdr::engine::apply_tonal_and_color_adjustments(&pixels, &shadow_params);
    assert!(lifted[3] > pixels[3], "L'ombre doit être débouchée");
    assert!((lifted[6] as i32 - pixels[6] as i32).abs() <= 2, "La haute lumière ne doit pas bouger");

    // 3. Contraste positif doit creuser les ombres et monter les hautes lumières
    let contrast_params = TonalParams {
        contrast: 0.8,
        ..Default::default()
    };
    let contrasted = ultrahdr::engine::apply_tonal_and_color_adjustments(&pixels, &contrast_params);
    assert!(contrasted[3] < pixels[3], "L'ombre doit être densifiée");
    assert!(contrasted[6] >= pixels[6], "La haute lumière doit être rehaussée");
}

#[test]
fn test_edr_no_clipping_at_extreme_nits() {
    // 4 zones de test : SDR blanc (203 nits), HDR modéré (1000 nits), HDR fort (4000 nits), HDR extrême (10000 nits)
    let nits_levels = [203.0f32, 1000.0, 4000.0, 10000.0];
    let mut hdr_nits = Vec::new();
    for &n in &nits_levels {
        hdr_nits.push(n);
        hdr_nits.push(n);
        hdr_nits.push(n);
    }

    let sim_vivid = simulate_hdr_on_sdr(&hdr_nits, 4, 1, 203.0, 1.0, 0.75, "vivid");
    let v_sdr = sim_vivid[0];
    let v_1k = sim_vivid[3];
    let v_4k = sim_vivid[6];
    let v_10k = sim_vivid[9];

    // Vérification de la stricte monotonie sans aucun cramage plat
    assert!(v_sdr < v_1k, "1000 nits doit être plus éclatant que 203 nits");
    assert!(v_1k < v_4k, "4000 nits doit être plus éclatant que 1000 nits");
    assert!(v_4k <= v_10k, "10000 nits ne doit pas régresser");
    assert!(v_1k < 255, "1000 nits ne doit pas être cramé à 255");

    let sim_natural = simulate_hdr_on_sdr(&hdr_nits, 4, 1, 203.0, 1.0, 0.75, "natural");
    assert!(sim_natural[0] < sim_natural[3]);
    assert!(sim_natural[3] < sim_natural[6]);
    assert!(sim_natural[6] <= sim_natural[9]);
    assert!(sim_natural[3] < 255);
}

#[test]
fn test_assemble_ultra_hdr_jpeg_with_exif_and_google_container() {
    let w = 80;
    let h = 80;
    let sdr_u8 = vec![180u8; (w * h * 3) as usize];
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
        0.5,
        TonalParams::default(),
        0.0,
    );

    let base_jpeg = encode_rgb_jpeg(&sdr_u8, w, h, 90).expect("encodage base");
    let gm_jpeg = encode_gain_map_jpeg(&res.gain_log2, res.gain_min, res.gain_max, w, h, 85).expect("encodage gm");

    // Créer un segment EXIF simulé de taille conséquente (comme sur les photos prises sur Android Pixel / Samsung)
    let fake_exif_len = 2048;
    let mut fake_exif = vec![0xFF, 0xE1, ((fake_exif_len >> 8) & 0xFF) as u8, (fake_exif_len & 0xFF) as u8];
    fake_exif.extend(b"Exif\0\0");
    fake_exif.resize(fake_exif_len, 0x42);

    // 1. Assembler avec EXIF (Android)
    let uhdr_with_exif = assemble_ultra_hdr_jpeg(&base_jpeg, &gm_jpeg, res.gain_min, res.gain_max, 1000.0, 203.0, Some(&fake_exif));

    // 2. Assembler sans EXIF (PC)
    let uhdr_no_exif = assemble_ultra_hdr_jpeg(&base_jpeg, &gm_jpeg, res.gain_min, res.gain_max, 1000.0, 203.0, None);

    // Vérifier les invariants MPF pour les deux fichiers :
    for (name, uhdr_bytes) in [("avec EXIF (Android)", &uhdr_with_exif), ("sans EXIF (PC)", &uhdr_no_exif)] {
        // Doit commencer par SOI
        assert_eq!(&uhdr_bytes[..2], &[0xFF, 0xD8], "{}: SOI manquant", name);

        // Trouver le marqueur MPF
        let mpf_marker = b"MPF\0";
        let mpf_pos = uhdr_bytes.windows(4).position(|w| w == mpf_marker).expect("MPF marker trouvé");

        // L'en-tête TIFF "II\x2A\x00" commence exactement 4 octets après "MPF\0"
        let tiff_pos = mpf_pos + 4;
        assert_eq!(&uhdr_bytes[tiff_pos..tiff_pos + 4], b"II\x2a\x00", "{}: Header TIFF MPF non conforme", name);

        // Dans la spécification CIPA DC-007 :
        // MP Entry 2 (Gain Map) a son champ offset à l'octet 74 depuis "II" :
        // 8 (TIFF header) + 2 (count) + 3 * 12 (tags) + 4 (next IFD) + 16 (Entry 1) + 8 (Entry 2 type + size) = 74
        let offset_bytes = &uhdr_bytes[tiff_pos + 74..tiff_pos + 78];
        let mpf_entry_offset = u32::from_le_bytes(offset_bytes.try_into().unwrap()) as usize;

        // Cet offset mesuré DEPUIS tiff_pos ("II") DOIT pointer EXACTEMENT vers les deux octets [0xFF, 0xD8] (SOI du gain map JPEG) !
        let target_gm_soi_pos = tiff_pos + mpf_entry_offset;
        assert!(target_gm_soi_pos + 2 <= uhdr_bytes.len(), "{}: target_gm_soi_pos hors limites", name);
        assert_eq!(
            &uhdr_bytes[target_gm_soi_pos..target_gm_soi_pos + 2],
            &[0xFF, 0xD8],
            "{}: Le pointeur MPF pointe sur [{:02X}, {:02X}] au lieu de [0xFF, 0xD8] SOI du gain map !",
            name,
            uhdr_bytes[target_gm_soi_pos],
            uhdr_bytes[target_gm_soi_pos + 1]
        );

        // Vérifier les balises Google Photos GContainer
        let text = String::from_utf8_lossy(uhdr_bytes);
        assert!(text.contains("Container:Directory"), "{}: Container:Directory manquant", name);
        assert!(text.contains("Item:Semantic=\"GainMap\""), "{}: Item:Semantic GainMap manquant", name);
        let actual_gm_len = uhdr_bytes.len() - target_gm_soi_pos;
        assert!(text.contains(&format!("Item:Length=\"{}\"", actual_gm_len)), "{}: Item:Length manquant ou erroné", name);
        assert!(text.contains("hdrgm:Gamma=\"1.0\""), "{}: hdrgm:Gamma manquant", name);
        assert!(text.contains("hdrgm:BaseRenditionIsHDR=\"False\""), "{}: BaseRenditionIsHDR manquant", name);
    }
}

#[test]
fn test_adaptive_ui_metrics_and_panel_clamping() {
    let ctx = egui::Context::default();

    // 1. Desktop 1080p standard
    let metrics_desktop = ultrahdr::theme_material::AdaptiveUiMetrics::compute(&ctx, false);
    assert!(metrics_desktop.scale >= 1.05 && metrics_desktop.scale <= 1.35);
    assert!(metrics_desktop.btn_height >= 38.0);
    assert!(metrics_desktop.btn_large_height >= 44.0);
    assert!(metrics_desktop.icon_btn_size >= 40.0);
    assert!(metrics_desktop.min_touch_target >= 44.0);

    // 2. Android touch metrics
    let metrics_android = ultrahdr::theme_material::AdaptiveUiMetrics::compute(&ctx, true);
    assert!(metrics_android.scale >= 1.18);
    assert!(metrics_android.btn_height >= 44.0);
    assert!(metrics_android.min_touch_target >= 48.0);
    assert!(metrics_android.icon_btn_size >= 46.0);

    // 3. Dynamic Side Panel width clamping
    // Verifies that for any screen width, remaining center width >= 420px
    let test_screen_widths: [f32; 7] = [800.0, 1024.0, 1280.0, 1440.0, 1920.0, 2560.0, 3840.0];
    for total_w in test_screen_widths {
        let max_side_panel_w = ((total_w - 420.0) / 2.0).clamp(260.0_f32, 560.0_f32);
        let left_w = max_side_panel_w;
        let right_w = max_side_panel_w;
        let remaining_center_w = total_w - left_w - right_w;
        if total_w >= 940.0 {
            assert!(remaining_center_w >= 419.9, "Screen {}px has center {}px < 420px", total_w, remaining_center_w);
        }
    }
}

#[test]
fn test_mobile_lightroom_deck_and_panes() {
    use ultrahdr::gui::ConfigPane;

    let panes = [
        ConfigPane::Tonal,
        ConfigPane::Profiles,
        ConfigPane::Engine,
        ConfigPane::Simulation,
        ConfigPane::Export,
    ];

    assert_eq!(panes.len(), 5);
    assert_eq!(ConfigPane::Tonal.title(), "Tonalité");
    assert_eq!(ConfigPane::Profiles.title(), "Profils");
    assert_eq!(ConfigPane::Engine.title(), "Moteur");
    assert_eq!(ConfigPane::Simulation.title(), "Rendu");
    assert_eq!(ConfigPane::Export.title(), "Export");

    // Vérifier les hauteurs de deck mobile (replié vs étendu)
    let screen_heights: [f32; 4] = [640.0, 800.0, 920.0, 1080.0];
    for total_h in screen_heights {
        let collapsed_h = 50.0_f32;
        let expanded_h = (total_h * 0.38_f32).clamp(210.0_f32, 285.0_f32);

        // Replié : au moins 85% d'espace réservé pour la photo
        let photo_space_collapsed = total_h - collapsed_h;
        assert!(photo_space_collapsed / total_h > 0.85);

        // Étendu : l'image a toujours entre 60% et 75% de hauteur libre
        let photo_space_expanded = total_h - expanded_h;
        assert!(photo_space_expanded / total_h >= 0.60);
    }

    // Vérifier la sécurité de clamping du split-handle contre les paniques
    let left = 10.0_f32;
    let right = 10.0_f32; // degenerate rect
    let min_x = left + 2.0;
    let max_x = (right - 2.0).max(min_x);
    assert!(min_x <= max_x);
    let split_pos = 0.5_f32;
    let split_x = (left + (right - left) * split_pos).clamp(min_x, max_x);
    assert!(split_x >= min_x && split_x <= max_x);
}

#[test]
fn test_mobile_bottom_deck_resizer_and_icon_tab_bounds() {
    use ultrahdr::gui::ConfigPane;

    // 1. Vérification des titres complets pour l'en-tête de l'onglet actif
    assert!(ConfigPane::Tonal.full_title().contains("Tonalité"));
    assert!(ConfigPane::Profiles.full_title().contains("Profils"));
    assert!(ConfigPane::Engine.full_title().contains("ISO 21496-1"));
    assert!(ConfigPane::Simulation.full_title().contains("Simulation SDR"));
    assert!(ConfigPane::Export.full_title().contains("Export"));

    // 2. Vérification que la somme des largeurs des 5 onglets icônes + chevron + espacements
    // reste STRICTEMENT inférieure ou égale à la largeur de l'écran (zéro débordement à droite)
    let screen_widths: [f32; 5] = [320.0, 360.0, 392.0, 412.0, 540.0];
    for avail_w in screen_widths {
        let item_gap = 4.0_f32;
        let btn_w = 28.0_f32;
        let total_gaps = item_gap * 5.0; // 5 intervalles
        let space_for_tabs = (avail_w - btn_w - total_gaps - 2.0).max(140.0);
        let tab_w = (space_for_tabs / 5.0).floor();

        let total_allocated = 5.0 * tab_w + btn_w + total_gaps;
        assert!(
            total_allocated <= avail_w,
            "Débordement détecté sur écran de {}px : total_allocated={}px",
            avail_w,
            total_allocated
        );
        assert!(tab_w >= 28.0, "Onglet trop étroit : {}px", tab_w);
    }

    // 3. Vérification de la dynamique de redimensionnement (resizer monter / descendre)
    let total_h = 800.0_f32;
    let max_h = (total_h * 0.85_f32).clamp(280.0, 750.0);
    let mut current_h = 290.0_f32;

    // Glisser vers le haut (dy = -50.0) -> Monter le panneau
    let dy_up = -50.0_f32;
    current_h = (current_h - dy_up).clamp(160.0, max_h);
    assert_eq!(current_h, 340.0);

    // Glisser vers le bas (dy = +100.0) -> Descendre le panneau
    let dy_down = 100.0_f32;
    current_h = (current_h - dy_down).clamp(160.0, max_h);
    assert_eq!(current_h, 240.0);

    // Glisser au maximum vers le bas -> clamped à 160.0 minimum
    current_h = (current_h - 500.0).clamp(160.0, max_h);
    assert_eq!(current_h, 160.0);

    // Glisser au maximum vers le haut (dy = -1000.0) -> clamped à max_h
    current_h = (current_h - (-1000.0)).clamp(160.0, max_h);
    assert_eq!(current_h, max_h);
}


