package android.view;

import android.content.Context;
import android.content.ContextWrapper;
import android.content.res.AssetManager;
import android.content.res.Configuration;
import android.content.res.Resources;

/** A context with its own theme: the base context's theme plus a style applied on top. */
public class ContextThemeWrapper extends ContextWrapper {
    private int themeResId;
    private Resources.Theme theme;
    private LayoutInflater inflater;
    private Configuration overrideConfiguration;
    public ContextThemeWrapper() { super(null); }
    public ContextThemeWrapper(Context base, int themeResId) { super(base); this.themeResId = themeResId; }
    public ContextThemeWrapper(Context base, Resources.Theme theme) { super(base); this.theme = theme; }
    protected void attachBaseContext(Context base) { super.attachBaseContext(base); }
    public void applyOverrideConfiguration(Configuration c) {
        if (overrideConfiguration != null) throw new IllegalStateException("Override configuration has already been set");
        overrideConfiguration = new Configuration(c);
    }
    public Configuration indroidOverrideConfiguration() { return overrideConfiguration; }
    public Resources getResources() { return getBaseContext().getResources(); }
    public AssetManager getAssets() { return getResources().getAssets(); }
    public void setTheme(int resid) {
        if (themeResId != resid) {
            themeResId = resid;
            if (theme != null) onApplyThemeResource(theme, resid, false);
        }
    }
    public void setTheme(Resources.Theme t) { theme = t; }
    public int getThemeResId() { return themeResId; }
    public Resources.Theme getTheme() {
        if (theme == null) {
            theme = getResources().newTheme();
            Context base = getBaseContext();
            Resources.Theme bt = base == null ? null : base.getTheme();
            if (bt != null) theme.setTo(bt);
            onApplyThemeResource(theme, themeResId, true);
        }
        return theme;
    }
    protected void onApplyThemeResource(Resources.Theme t, int resid, boolean first) { if (resid != 0) t.applyStyle(resid, true); }
    public Object getSystemService(String name) {
        if (LAYOUT_INFLATER_SERVICE.equals(name)) {
            if (inflater == null) inflater = LayoutInflater.from(getBaseContext()).cloneInContext(this);
            return inflater;
        }
        return getBaseContext().getSystemService(name);
    }
}
