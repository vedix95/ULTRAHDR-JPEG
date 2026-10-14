package com.ultrahdr.studio;

import android.app.NativeActivity;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.util.Log;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;

public class MainActivity extends NativeActivity {
    private static final String TAG = "UltraHDR_MainActivity";
    private static final int PICK_IMAGE_REQUEST = 1001;
    private static MainActivity instance;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        instance = this;
        Log.i(TAG, "UltraHDR Studio MainActivity démarré avec succès.");
    }

    @Override
    protected void onDestroy() {
        if (instance == this) {
            instance = null;
        }
        super.onDestroy();
    }

    /**
     * Ouvre le sélecteur Android natif de photos (Galerie, Google Photos, Fichiers, Téléchargements).
     * Appelable directement depuis Rust via JNI.
     */
    public static void openImagePicker() {
        if (instance == null) {
            Log.e(TAG, "Impossible d'ouvrir le sélecteur: instance MainActivity est null");
            return;
        }
        instance.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    Intent intent = new Intent(Intent.ACTION_GET_CONTENT);
                    intent.setType("image/*");
                    intent.addCategory(Intent.CATEGORY_OPENABLE);
                    intent.putExtra(Intent.EXTRA_MIME_TYPES, new String[] {
                        "image/jpeg",
                        "image/png",
                        "image/webp",
                        "image/vnd.radiance",
                        "image/x-exr",
                        "image/tiff",
                        "image/bmp"
                    });
                    instance.startActivityForResult(
                        Intent.createChooser(intent, "Choisir une photo pour UltraHDR Studio"),
                        PICK_IMAGE_REQUEST
                    );
                    Log.i(TAG, "Sélecteur de photos Android lancé.");
                } catch (Exception e) {
                    Log.e(TAG, "Erreur au lancement du sélecteur: " + e.getMessage());
                }
            }
        });
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == PICK_IMAGE_REQUEST && resultCode == RESULT_OK && data != null && data.getData() != null) {
            Uri uri = data.getData();
            Log.i(TAG, "Image sélectionnée par l'utilisateur: " + uri.toString());
            try {
                InputStream is = getContentResolver().openInputStream(uri);
                if (is != null) {
                    File cacheDir = getCacheDir();
                    File outFile = new File(cacheDir, "imported_photo.jpg");
                    FileOutputStream fos = new FileOutputStream(outFile);
                    byte[] buf = new byte[32768];
                    int len;
                    while ((len = is.read(buf)) > 0) {
                        fos.write(buf, 0, len);
                    }
                    fos.close();
                    is.close();

                    Log.i(TAG, "Photo écrite dans le cache: " + outFile.getAbsolutePath() + " (" + outFile.length() + " octets)");

                    // Création du drapeau pour notifier Rust
                    File flag = new File(cacheDir, "photo_ready.flag");
                    flag.createNewFile();
                }
            } catch (Exception e) {
                Log.e(TAG, "Erreur lors de la lecture de l'image sélectionnée: " + e.getMessage());
            }
        }
    }
}
