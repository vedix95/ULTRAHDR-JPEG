import unittest
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import numpy as np
from PIL import Image
import io
import tempfile
import cv2

from core.image_loader import load_image, LoadedImage, srgb_to_linear, linear_to_srgb
from core.analyzer import analyze_image_potential
from core.ultrahdr_engine import (
    compute_inverse_tone_map, compute_gain_map, assemble_ultra_hdr_jpeg,
    simulate_hdr_on_sdr, generate_nits_heatmap, encode_gain_map_jpeg
)
from core.display_detector import get_display_hdr_status


class TestUltraHDRStudio(unittest.TestCase):
    def setUp(self):
        self.temp_dir = Path(tempfile.mkdtemp(prefix="test_uhdr_"))

    def tearDown(self):
        import shutil
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_display_detector(self):
        status = get_display_hdr_status()
        self.assertIn("has_hdr_enabled", status)
        self.assertIn("has_hdr_supported", status)
        self.assertIn("recommended_mode", status)
        self.assertIn(status["recommended_mode"], ["true_hdr", "simulation_sdr"])

    def test_image_loader_sdr(self):
        # Création d'une image SDR PNG test
        img_arr = np.zeros((120, 160, 3), dtype=np.uint8)
        img_arr[:60, :] = [250, 180, 40]  # Ciel doré
        img_arr[60:, :] = [20, 30, 40]    # Sol sombre
        test_file = self.temp_dir / "test_sdr.png"
        Image.fromarray(img_arr).save(test_file)

        loaded = load_image(test_file)
        self.assertEqual(loaded.width, 160)
        self.assertEqual(loaded.height, 120)
        self.assertEqual(loaded.source_bit_depth, 8)
        self.assertFalse(loaded.is_native_hdr)
        self.assertEqual(loaded.sdr_uint8.shape, (120, 160, 3))
        self.assertEqual(loaded.linear_rgb.shape, (120, 160, 3))

    def test_image_loader_16bit_tiff(self):
        # Création d'un TIFF 16-bit
        tiff_arr = np.full((80, 80, 3), 50000, dtype=np.uint16)
        test_file = self.temp_dir / "test_16.tif"
        cv2.imwrite(str(test_file), cv2.cvtColor(tiff_arr, cv2.COLOR_RGB2BGR))

        loaded = load_image(test_file)
        self.assertEqual(loaded.width, 80)
        self.assertEqual(loaded.height, 80)
        self.assertEqual(loaded.source_bit_depth, 16)
        self.assertTrue(loaded.is_native_hdr)

    def test_analyzer_classification(self):
        # Création d'une scène avec coucher de soleil
        img_arr = np.zeros((150, 150, 3), dtype=np.uint8)
        img_arr[:75, :] = [255, 170, 40]   # Coucher de soleil
        img_arr[75:, :] = [30, 25, 20]     # Silhouette
        test_file = self.temp_dir / "sunset.jpg"
        Image.fromarray(img_arr).save(test_file)

        loaded = load_image(test_file)
        res = analyze_image_potential(loaded)
        self.assertGreater(res.potential_score, 70)
        self.assertIn(res.category, ["SUNSET", "LANDSCAPE", "NIGHT_NEON", "STUDIO"])
        self.assertIsNotNone(res.recommended_preset)

    def test_gain_map_computation(self):
        # Vérification mathématique du gain map
        sdr_lin = np.full((50, 50, 3), 0.5, dtype=np.float32)
        # Élever à 1000 nits avec sdr_white=203 nits
        hdr_nits = np.full((50, 50, 3), 1000.0, dtype=np.float32)

        gain_log2, gmin, gmax = compute_gain_map(sdr_lin, hdr_nits, sdr_white=203.0)
        self.assertEqual(gmin, 0.0)
        # Ratio HDR/SDR = (1000/203) / 0.5 = 9.85 -> log2 ~ 3.3
        self.assertGreater(gmax, 2.0)
        self.assertEqual(gain_log2.shape, (50, 50))

    def test_assemble_ultra_hdr_jpeg(self):
        # Vérification du fichier binaire assemblé
        sdr_u8 = np.zeros((100, 100, 3), dtype=np.uint8)
        sdr_u8[:, :50] = [40, 40, 40]
        sdr_u8[:, 50:] = [240, 240, 240]

        sdr_lin = srgb_to_linear(sdr_u8.astype(np.float32) / 255.0)
        hdr_nits = compute_inverse_tone_map(sdr_lin, sdr_white=203.0, peak_nits=1200.0)
        gain_log2, gmin, gmax = compute_gain_map(sdr_lin, hdr_nits, sdr_white=203.0)

        uhdr_bytes = assemble_ultra_hdr_jpeg(sdr_u8, gain_log2, gmin, gmax, 1200.0, 203.0)
        self.assertGreater(len(uhdr_bytes), 1000)

        # Doit commencer par le marqueur SOI JPEG (0xFF 0xD8)
        self.assertEqual(uhdr_bytes[:2], b'\xff\xd8')

        # Doit contenir le marqueur XMP hdrgm Google / Adobe
        self.assertIn(b'http://ns.adobe.com/hdr-gain-map/1.0/', uhdr_bytes)
        self.assertIn(b'hdrgm:GainMapMax', uhdr_bytes)

        # Doit contenir l'en-tête MPF CIPA DC-007
        self.assertIn(b'MPF\x00', uhdr_bytes)

    def test_sdr_simulation(self):
        # Test simulation ACES, Reinhard et décalage EV
        hdr_nits = np.full((60, 60, 3), 800.0, dtype=np.float32)
        sim_aces = simulate_hdr_on_sdr(hdr_nits, sdr_white=203.0, ev_shift=-1.0, tone_map_mode="aces")
        self.assertEqual(sim_aces.shape, (60, 60, 3))
        self.assertEqual(sim_aces.dtype, np.uint8)

        sim_reinhard = simulate_hdr_on_sdr(hdr_nits, sdr_white=203.0, ev_shift=0.0, tone_map_mode="reinhard")
        self.assertEqual(sim_reinhard.shape, (60, 60, 3))
        self.assertEqual(sim_reinhard.dtype, np.uint8)

        heat = generate_nits_heatmap(hdr_nits, sdr_white=203.0, peak_nits=1200.0)
        self.assertEqual(heat.shape, (60, 60, 3))
        self.assertEqual(heat.dtype, np.uint8)

    def test_raw_gain_map_not_all_white(self):
        # Simulation d'un fichier DNG / Camera RAW avec hautes lumières et ombres
        from core.ultrahdr_engine import compute_hdr_and_gain_map
        h, w = 100, 100
        sdr_u8 = np.zeros((h, w, 3), dtype=np.uint8)
        sdr_u8[:50, :] = [30, 35, 40]      # Ombres/tons sombres
        sdr_u8[50:, :] = [240, 240, 230]   # Ciel lumineux

        sdr_lin = srgb_to_linear(sdr_u8.astype(np.float32) / 255.0)
        # Données de capteur avec réserve dans les hautes lumières
        raw_lin = sdr_lin.copy()
        raw_lin[50:, :] = sdr_lin[50:, :] * 3.0

        hdr_nits, gain_log2, gmin, gmax = compute_hdr_and_gain_map(
            sdr_uint8=sdr_u8,
            hdr_lin_input=raw_lin,
            is_native_hdr=True,
            peak_nits=1200.0,
            sdr_white=203.0
        )

        gain_range = max(gmax - gmin, 1e-4)
        gain_u8 = np.clip((gain_log2 - gmin) / gain_range * 255.0 + 0.5, 0, 255).astype(np.uint8)

        # La gain map ne doit PAS être toute blanche
        self.assertLess(np.mean(gain_u8[:50, :]), 40, "Les ombres doivent avoir un gain proche de zéro")
        self.assertGreater(np.mean(gain_u8[50:, :]), 180, "Les hautes lumières doivent avoir un fort gain")
        self.assertGreater(np.std(gain_u8), 50.0, "La gain map doit avoir du contraste et du relief")

    def test_exr_4d_multipart_handling(self):
        # Simulation d'un fichier EXR multi-calque 4D (1, 512, 1024, 3)
        from core.ultrahdr_engine import compute_hdr_and_gain_map
        from core.image_loader import normalize_image_array

        exr_4d = np.random.uniform(0.0, 5.0, (1, 512, 1024, 3)).astype(np.float32)
        normalized = normalize_image_array(exr_4d)
        self.assertEqual(normalized.shape, (512, 1024, 3))

        sdr_base = np.random.randint(0, 255, (512, 1024, 3), dtype=np.uint8)
        hdr_nits, gain_log2, gmin, gmax = compute_hdr_and_gain_map(
            sdr_uint8=sdr_base,
            hdr_lin_input=exr_4d,
            is_native_hdr=True,
            peak_nits=1000.0
        )
        self.assertEqual(hdr_nits.shape, (512, 1024, 3))
        self.assertEqual(gain_log2.shape, (512, 1024))

    def test_contrast_parameter(self):
        from core.ultrahdr_engine import compute_hdr_and_gain_map
        sdr_base = np.full((60, 60, 3), 200, dtype=np.uint8)
        sdr_base[:30, :] = 40

        _, gain_neutral, _, max_neutral = compute_hdr_and_gain_map(sdr_base, peak_nits=1200.0, contrast=0.0)
        _, gain_contrast, _, max_contrast = compute_hdr_and_gain_map(sdr_base, peak_nits=1200.0, contrast=0.5)

        self.assertGreaterEqual(max_contrast, max_neutral)

    def test_vibrance_saturation_parameter(self):
        from core.ultrahdr_engine import apply_vibrance_saturation, compute_hdr_and_gain_map
        test_img = np.array([[[120, 120, 120], [200, 150, 120], [255, 0, 0]]], dtype=np.uint8)
        
        sat_boost = apply_vibrance_saturation(test_img, saturation=0.5, vibrance=0.0)
        self.assertEqual(sat_boost[0, 0, 0], 120)
        self.assertGreaterEqual(sat_boost[0, 1, 0], test_img[0, 1, 0])

        vib_boost = apply_vibrance_saturation(test_img, saturation=0.0, vibrance=0.5)
        self.assertEqual(vib_boost[0, 0, 0], 120)
        self.assertEqual(vib_boost[0, 2, 0], 255)

        # Integration with compute_hdr_and_gain_map
        sdr_base = np.full((40, 40, 3), 180, dtype=np.uint8)
        hdr, gm, gmin, gmax = compute_hdr_and_gain_map(sdr_base, saturation=0.2, vibrance=0.3)
        self.assertEqual(hdr.shape, (40, 40, 3))
        self.assertEqual(gm.shape, (40, 40))


if __name__ == "__main__":
    unittest.main()

