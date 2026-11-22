package android.app;

import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.res.Resources;
import android.os.Looper;

final class ContextImpl extends Context {
    static final Resources RES = Resources.getSystem();
    Application app;
    public Resources getResources() { return RES; }
    public String getPackageName() { return ActivityThread.packageName; }
    public Context getApplicationContext() { return app != null ? app : this; }
    public SharedPreferences getSharedPreferences(String name, int mode) { return VMPrefs.get(name); }
    private Resources.Theme theme;
    private int themeRes;
    private android.view.LayoutInflater inflater;
    public Resources.Theme getTheme() {
        if (theme == null) {
            theme = RES.newTheme();
            int t = themeRes != 0 ? themeRes : ActivityThread.nativeTheme("");
            theme.applyStyle(t != 0 ? t : INDROID_DEFAULT_THEME, true);
        }
        return theme;
    }
    public void setTheme(int resid) { themeRes = resid; if (theme != null) theme.applyStyle(resid, true); }
    public Object getSystemService(String name) {
        if (LAYOUT_INFLATER_SERVICE.equals(name)) { if (inflater == null) inflater = android.view.LayoutInflater.indroidCreate(this); return inflater; }
        if (INPUT_METHOD_SERVICE.equals(name)) return new android.view.inputmethod.InputMethodManager();
        return null;
    }
    public void startActivity(Intent intent) { ActivityThread.startActivity(null, intent, -1); }
    public Looper getMainLooper() { return Looper.getMainLooper(); }
    public java.io.File getFilesDir() { return new java.io.File("/data/files"); }
    public java.io.File getCacheDir() { return new java.io.File("/data/cache"); }
}
