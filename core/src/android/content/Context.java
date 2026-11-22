package android.content;

import android.content.res.Resources;
import android.os.Looper;

public abstract class Context {
    public static final int MODE_PRIVATE = 0, MODE_APPEND = 0x8000;
    public static final String LAYOUT_INFLATER_SERVICE = "layout_inflater", INPUT_METHOD_SERVICE = "input_method", WINDOW_SERVICE = "window",
        CLIPBOARD_SERVICE = "clipboard", VIBRATOR_SERVICE = "vibrator", AUDIO_SERVICE = "audio", CONNECTIVITY_SERVICE = "connectivity",
        NOTIFICATION_SERVICE = "notification", ALARM_SERVICE = "alarm", SENSOR_SERVICE = "sensor", LOCATION_SERVICE = "location";
    public abstract Resources getResources();
    public abstract String getPackageName();
    public abstract Context getApplicationContext();
    public abstract SharedPreferences getSharedPreferences(String name, int mode);
    public abstract Object getSystemService(String name);
    public abstract void startActivity(Intent intent);
    public abstract Looper getMainLooper();
    public abstract java.io.File getFilesDir();
    public abstract java.io.File getCacheDir();
    public final String getString(int id) { return getResources().getString(id); }
    public final String getString(int id, Object... args) { return getResources().getString(id, args); }
    public final CharSequence getText(int id) { return getResources().getText(id); }
    public final int getColor(int id) { return getResources().getColor(id, getTheme()); }
    public final android.content.res.ColorStateList getColorStateList(int id) { return getResources().getColorStateList(id, getTheme()); }
    public final android.graphics.drawable.Drawable getDrawable(int id) { return getResources().getDrawable(id, getTheme()); }
    public abstract android.content.res.Resources.Theme getTheme();
    public abstract void setTheme(int resid);
    public final android.content.res.TypedArray obtainStyledAttributes(int[] attrs) { return getTheme().obtainStyledAttributes(attrs); }
    public final android.content.res.TypedArray obtainStyledAttributes(int resid, int[] attrs) { return getTheme().obtainStyledAttributes(resid, attrs); }
    public final android.content.res.TypedArray obtainStyledAttributes(android.util.AttributeSet set, int[] attrs) { return getTheme().obtainStyledAttributes(set, attrs, 0, 0); }
    public final android.content.res.TypedArray obtainStyledAttributes(android.util.AttributeSet set, int[] attrs, int defStyleAttr, int defStyleRes) { return getTheme().obtainStyledAttributes(set, attrs, defStyleAttr, defStyleRes); }
    /** android.R.style.Theme_Material_Light: used when the manifest names no theme. */
    public static final int INDROID_DEFAULT_THEME = 0x01030237;
    public ClassLoader getClassLoader() { return ClassLoader.getSystemClassLoader(); }
    public android.content.pm.ApplicationInfo getApplicationInfo() { return new android.content.pm.ApplicationInfo(); }
    public android.content.pm.PackageManager getPackageManager() { return new android.content.pm.PackageManager(); }
    public ContentResolver getContentResolver() { return new ContentResolver(); }
    public void sendBroadcast(Intent i) {}
    public ComponentName startService(Intent i) { return null; }
    public boolean stopService(Intent i) { return false; }
    public int checkSelfPermission(String p) { return android.content.pm.PackageManager.PERMISSION_DENIED; }
    public int checkPermission(String p, int pid, int uid) { return android.content.pm.PackageManager.PERMISSION_DENIED; }
    public int checkCallingOrSelfPermission(String p) { return android.content.pm.PackageManager.PERMISSION_DENIED; }
    public void enforceCallingOrSelfPermission(String p, String msg) { throw new SecurityException(msg); }
    /** Configuration changes are not applied separately; the same resources serve every context. */
    public Context createConfigurationContext(android.content.res.Configuration c) {
        Context app = getApplicationContext();
        final Resources r = app != null && app != this ? app.getResources() : Resources.getSystem();
        return new ContextWrapper(this) { public Resources getResources() { return r; } };
    }
    public Context createDeviceProtectedStorageContext() { return this; }
    public Context createContextForSplit(String name) { return this; }
    public Context createAttributionContext(String tag) { return this; }
    public Context createWindowContext(int type, android.os.Bundle options) { return this; }
    public Context createPackageContext(String pkg, int flags) throws android.content.pm.PackageManager.NameNotFoundException { return this; }
    public boolean isDeviceProtectedStorage() { return false; }
    public boolean isRestricted() { return false; }
    public boolean isUiContext() { return true; }
    public String getOpPackageName() { return getPackageName(); }
    public String getAttributionTag() { return null; }
    public String getPackageCodePath() { return "/data/app/" + getPackageName() + "/base.apk"; }
    public String getPackageResourcePath() { return getPackageCodePath(); }
    public android.content.res.AssetManager getAssets() { return getResources().getAssets(); }
    public java.io.File getCodeCacheDir() { return new java.io.File(getCacheDir(), "code_cache"); }
    public java.io.File getNoBackupFilesDir() { return new java.io.File(getFilesDir().getParentFile(), "no_backup"); }
    public java.io.File getDataDir() { return getFilesDir().getParentFile(); }
    public java.io.File getExternalFilesDir(String type) { return null; }
    public java.io.File[] getExternalFilesDirs(String type) { return new java.io.File[0]; }
    public java.io.File getExternalCacheDir() { return null; }
    public java.io.File getObbDir() { return null; }
    public java.io.File getDir(String name, int mode) { return new java.io.File(getDataDir(), "app_" + name); }
    public java.io.File getDatabasePath(String name) { return new java.io.File(new java.io.File(getDataDir(), "databases"), name); }
    public java.io.File getFileStreamPath(String name) { return new java.io.File(getFilesDir(), name); }
    public String[] fileList() { return new String[0]; }
    public boolean deleteFile(String name) { return false; }
    public java.io.FileInputStream openFileInput(String name) throws java.io.FileNotFoundException { throw new java.io.FileNotFoundException(name); }
    public boolean deleteSharedPreferences(String name) { return true; }
    public boolean moveSharedPreferencesFrom(Context c, String name) { return true; }
    public final <T> T getSystemService(Class<T> c) { String n = getSystemServiceName(c); return n == null ? null : c.cast(getSystemService(n)); }
    public String getSystemServiceName(Class<?> c) {
        String n = c.getName();
        if (n.equals("android.view.LayoutInflater")) return LAYOUT_INFLATER_SERVICE;
        if (n.equals("android.view.inputmethod.InputMethodManager")) return INPUT_METHOD_SERVICE;
        if (n.equals("android.view.WindowManager")) return WINDOW_SERVICE;
        if (n.equals("android.content.ClipboardManager")) return CLIPBOARD_SERVICE;
        if (n.equals("android.app.NotificationManager")) return NOTIFICATION_SERVICE;
        if (n.equals("android.app.AlarmManager")) return ALARM_SERVICE;
        if (n.equals("android.view.accessibility.AccessibilityManager")) return "accessibility";
        if (n.equals("android.app.UiModeManager")) return "uimode";
        if (n.equals("android.os.PowerManager")) return "power";
        if (n.equals("android.app.LocaleManager")) return "locale";
        if (n.equals("android.app.ActivityManager")) return "activity";
        return null;
    }
    public Intent registerReceiver(BroadcastReceiver r, IntentFilter f) { return null; }
    public Intent registerReceiver(BroadcastReceiver r, IntentFilter f, int flags) { return null; }
    public Intent registerReceiver(BroadcastReceiver r, IntentFilter f, String perm, android.os.Handler h) { return null; }
    public Intent registerReceiver(BroadcastReceiver r, IntentFilter f, String perm, android.os.Handler h, int flags) { return null; }
    public void unregisterReceiver(BroadcastReceiver r) {}
    public void sendBroadcast(Intent i, String perm) {}
    public void sendOrderedBroadcast(Intent i, String perm) {}
    public void startActivity(Intent intent, android.os.Bundle options) { startActivity(intent); }
    public void startActivities(Intent[] intents) { for (Intent i : intents) startActivity(i); }
    public void startActivities(Intent[] intents, android.os.Bundle options) { startActivities(intents); }
    public ComponentName startForegroundService(Intent i) { return null; }
    public boolean bindService(Intent i, ServiceConnection c, int flags) { return false; }
    public void unbindService(ServiceConnection c) {}
    public void grantUriPermission(String pkg, android.net.Uri uri, int flags) {}
    public void revokeUriPermission(android.net.Uri uri, int flags) {}
    public android.view.Display getDisplay() { return null; }
    public int getUserId() { return 0; }
    public java.util.concurrent.Executor getMainExecutor() { return new java.util.concurrent.Executor() { public void execute(Runnable r) { new android.os.Handler(android.os.Looper.getMainLooper()).post(r); } }; }
    public void registerComponentCallbacks(ComponentCallbacks c) {}
    public void unregisterComponentCallbacks(ComponentCallbacks c) {}
    public int getWallpaperDesiredMinimumWidth() { return 0; }
 public java.io.FileOutputStream openFileOutput(String name,int mode) throws java.io.FileNotFoundException { throw new java.io.FileNotFoundException(name); }
}
