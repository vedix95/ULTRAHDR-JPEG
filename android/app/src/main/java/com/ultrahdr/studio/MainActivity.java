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
        Log.i(TAG, "UltraHDR Studio MainActivity onCreate démarré avec succès.");
        handleIncomingIntent(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        Log.i(TAG, "UltraHDR Studio MainActivity onNewIntent reçu.");
        handleIncomingIntent(intent);
    }

    @Override
    protected void onDestroy() {
        if (instance == this) {
            instance = null;
        }
        super.onDestroy();
    }

    /**
     * Traite les intents reçus lors d'un partage depuis une autre application (ACTION_SEND)
     * ou d'une ouverture directe de fichier (ACTION_VIEW).
     */
    private void handleIncomingIntent(Intent intent) {
        if (intent == null) return;
        String action = intent.getAction();
        String type = intent.getType();
        Log.i(TAG, "Traitement incoming intent: action=" + action + ", type=" + type);

        Uri uri = null;
        if (Intent.ACTION_SEND.equals(action)) {
            // Reçu depuis le menu 'Partager' d'une autre application (Google Photos, Galerie, etc.)
            uri = intent.getParcelableExtra(Intent.EXTRA_STREAM);
            if (uri == null && intent.getClipData() != null && intent.getClipData().getItemCount() > 0) {
                uri = intent.getClipData().getItemAt(0).getUri();
            }
        } else if (Intent.ACTION_VIEW.equals(action)) {
            // Reçu depuis "Ouvrir avec..." ou un explorateur de fichiers
            uri = intent.getData();
        }

        if (uri != null) {
            Log.i(TAG, "Image reçue via intent: " + uri);
            copyUriToCacheAsync(uri);
        }
    }

    /**
     * Méthode d'instance appelée directement depuis Rust via JNI.
     */
    public void openPicker() {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                openPickerImpl();
            }
        });
    }

    /**
     * Méthode statique de secours appelée depuis Rust via JNI si besoin.
     */
    public static void openImagePicker() {
        if (instance != null) {
            instance.openPicker();
        } else {
            Log.e(TAG, "Impossible d'ouvrir le sélecteur: instance null");
        }
    }

    private void openPickerImpl() {
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
                "image/bmp",
                "image/x-adobe-dng",
                "image/*"
            });
            startActivityForResult(
                Intent.createChooser(intent, "Sélectionner une photo pour UltraHDR"),
                PICK_IMAGE_REQUEST
            );
            Log.i(TAG, "Sélecteur de photos Android lancé.");
        } catch (Exception e) {
            Log.e(TAG, "Erreur au lancement du sélecteur: " + e.getMessage(), e);
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == PICK_IMAGE_REQUEST && resultCode == RESULT_OK && data != null) {
            Uri uri = data.getData();
            if (uri == null && data.getClipData() != null && data.getClipData().getItemCount() > 0) {
                uri = data.getClipData().getItemAt(0).getUri();
            }
            if (uri != null) {
                Log.i(TAG, "Photo sélectionnée dans le sélecteur: " + uri);
                copyUriToCacheAsync(uri);
            }
        }
    }

    private void copyUriToCacheAsync(final Uri uri) {
        new Thread(new Runnable() {
            @Override
            public void run() {
                try {
                    InputStream is = getContentResolver().openInputStream(uri);
                    if (is != null) {
                        File cacheDir = getCacheDir();
                        if (!cacheDir.exists()) {
                            cacheDir.mkdirs();
                        }
                        File outFile = new File(cacheDir, "imported_photo.jpg");
                        FileOutputStream fos = new FileOutputStream(outFile);
                        byte[] buf = new byte[65536];
                        int len;
                        while ((len = is.read(buf)) > 0) {
                            fos.write(buf, 0, len);
                        }
                        fos.flush();
                        fos.close();
                        is.close();

                        Log.i(TAG, "Photo écrite dans le cache: " + outFile.getAbsolutePath() + " (" + outFile.length() + " octets)");

                        // Signalement atomique à l'application Rust
                        File flag = new File(cacheDir, "photo_ready.flag");
                        flag.createNewFile();
                    }
                } catch (Exception e) {
                    Log.e(TAG, "Erreur lors de la copie de l'image: " + e.getMessage(), e);
                }
            }
        }).start();
    }
}
