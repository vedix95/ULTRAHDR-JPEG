package com.ultrahdr.studio;

import android.app.NativeActivity;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.Intent;
import android.media.MediaScannerConnection;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.provider.MediaStore;
import android.util.Log;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

public class MainActivity extends NativeActivity {
    private static final String TAG = "UltraHDR_MainActivity";
    private static final int PICK_IMAGE_REQUEST = 1001;
    private static MainActivity instance;

    // Dernier fichier exporté
    private static String lastExportedPath = null;
    private static Uri lastExportedUri = null;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        instance = this;
        try {
            android.os.StrictMode.setVmPolicy(new android.os.StrictMode.VmPolicy.Builder().build());
        } catch (Exception ignored) {}
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
            uri = intent.getParcelableExtra(Intent.EXTRA_STREAM);
            if (uri == null && intent.getClipData() != null && intent.getClipData().getItemCount() > 0) {
                uri = intent.getClipData().getItemAt(0).getUri();
            }
        } else if (Intent.ACTION_VIEW.equals(action)) {
            uri = intent.getData();
        }

        if (uri != null) {
            Log.i(TAG, "Image reçue via intent: " + uri);
            copyUriToCacheAsync(uri);
        }
    }

    /**
     * Méthode d'instance appelée directement depuis Rust via JNI pour le sélecteur.
     */
    public void openPicker() {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                openPickerImpl();
            }
        });
    }

    public static void openImagePicker() {
        if (instance != null) {
            instance.openPicker();
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
                        File flag = new File(cacheDir, "photo_ready.flag");
                        flag.createNewFile();
                    }
                } catch (Exception e) {
                    Log.e(TAG, "Erreur lors de la copie de l'image: " + e.getMessage(), e);
                }
            }
        }).start();
    }

    /**
     * Enregistre les octets de l'image Ultra HDR dans DCIM/ULTRAHDR-STUDIO.
     * Utilise MediaStore (Scoped Storage) compatible Android 10, 11, 12, 13, 14, 15.
     * Retourne le chemin absolu du fichier enregistré, ou null.
     */
    public String saveExportToDcim(byte[] data, String fileName) {
        try {
            ContentResolver resolver = getContentResolver();
            Uri imageUri = null;
            String relativeDir = Environment.DIRECTORY_DCIM + "/ULTRAHDR-STUDIO";

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                ContentValues values = new ContentValues();
                values.put(MediaStore.Images.Media.DISPLAY_NAME, fileName);
                values.put(MediaStore.Images.Media.MIME_TYPE, "image/jpeg");
                values.put(MediaStore.Images.Media.RELATIVE_PATH, relativeDir);
                values.put(MediaStore.Images.Media.IS_PENDING, 1);

                imageUri = resolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values);
                if (imageUri != null) {
                    OutputStream os = resolver.openOutputStream(imageUri);
                    if (os != null) {
                        os.write(data);
                        os.flush();
                        os.close();
                    }
                    values.clear();
                    values.put(MediaStore.Images.Media.IS_PENDING, 0);
                    resolver.update(imageUri, values, null, null);
                }
            } else {
                File dcimDir = new File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DCIM), "ULTRAHDR-STUDIO");
                if (!dcimDir.exists()) {
                    dcimDir.mkdirs();
                }
                File outFile = new File(dcimDir, fileName);
                FileOutputStream fos = new FileOutputStream(outFile);
                fos.write(data);
                fos.flush();
                fos.close();

                ContentValues values = new ContentValues();
                values.put(MediaStore.Images.Media.DATA, outFile.getAbsolutePath());
                values.put(MediaStore.Images.Media.MIME_TYPE, "image/jpeg");
                imageUri = resolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values);
            }

            File publicFile = new File("/sdcard/DCIM/ULTRAHDR-STUDIO", fileName);
            String finalPath = publicFile.getAbsolutePath();

            lastExportedPath = finalPath;
            lastExportedUri = imageUri;

            // Scanner le fichier pour que l'album apparaisse immédiatement dans la Galerie Android
            MediaScannerConnection.scanFile(this, new String[] { finalPath }, new String[] { "image/jpeg" }, null);

            Log.i(TAG, "Export Ultra HDR réussi dans DCIM/ULTRAHDR-STUDIO: " + finalPath + ", Uri: " + imageUri);
            return finalPath;
        } catch (Exception e) {
            Log.e(TAG, "Erreur saveExportToDcim: " + e.getMessage(), e);
            return null;
        }
    }

    /**
     * Notifie le MediaScanner d'Android si le fichier a été écrit directement par Rust.
     */
    public void notifyMediaScan(String filePath) {
        lastExportedPath = filePath;
        MediaScannerConnection.scanFile(this, new String[] { filePath }, new String[] { "image/jpeg" },
            new MediaScannerConnection.OnScanCompletedListener() {
                @Override
                public void onScanCompleted(String path, Uri uri) {
                    lastExportedUri = uri;
                    Log.i(TAG, "MediaScanner terminé pour: " + path + " -> " + uri);
                }
            });
    }

    /**
     * Ouvre la photo exportée dans l'application Galerie / Photos par défaut.
     */
    public void viewExportedImage() {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    Intent intent = new Intent(Intent.ACTION_VIEW);
                    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_GRANT_READ_URI_PERMISSION);
                    if (lastExportedUri != null) {
                        intent.setDataAndType(lastExportedUri, "image/jpeg");
                    } else if (lastExportedPath != null) {
                        File file = new File(lastExportedPath);
                        intent.setDataAndType(Uri.fromFile(file), "image/jpeg");
                    } else {
                        File file = new File("/sdcard/DCIM/ULTRAHDR-STUDIO");
                        intent.setDataAndType(Uri.fromFile(file), "image/*");
                    }
                    startActivity(Intent.createChooser(intent, "Ouvrir l'image Ultra HDR"));
                } catch (Exception e) {
                    Log.e(TAG, "Erreur viewExportedImage: " + e.getMessage(), e);
                }
            }
        });
    }

    /**
     * Ouvre le dossier DCIM/ULTRAHDR-STUDIO dans l'explorateur de fichiers.
     */
    public void viewExportedFolder() {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    Intent intent = new Intent(Intent.ACTION_VIEW);
                    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                    Uri folderUri = Uri.parse("content://com.android.externalstorage.documents/document/primary%3ADCIM%2FULTRAHDR-STUDIO");
                    intent.setDataAndType(folderUri, "resource/folder");
                    if (intent.resolveActivity(getPackageManager()) == null) {
                        intent.setDataAndType(folderUri, "*/*");
                    }
                    startActivity(Intent.createChooser(intent, "Ouvrir le dossier DCIM/ULTRAHDR-STUDIO"));
                } catch (Exception e) {
                    Log.w(TAG, "Tentative DocumentsProvider échouée: " + e.getMessage() + ", repli vers sélecteur...");
                    try {
                        Intent fallbackIntent = new Intent(Intent.ACTION_VIEW);
                        fallbackIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                        File folder = new File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DCIM), "ULTRAHDR-STUDIO");
                        fallbackIntent.setDataAndType(Uri.fromFile(folder), "*/*");
                        startActivity(Intent.createChooser(fallbackIntent, "Ouvrir le dossier DCIM/ULTRAHDR-STUDIO"));
                    } catch (Exception e2) {
                        Log.e(TAG, "Erreur viewExportedFolder: " + e2.getMessage(), e2);
                    }
                }
            }
        });
    }
}
