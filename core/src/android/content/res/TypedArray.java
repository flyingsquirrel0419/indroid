package android.content.res;

import android.graphics.drawable.Drawable;
import android.util.TypedValue;

/** Attribute values resolved by Theme.obtainStyledAttributes (encoded, see VMValue). */
public class TypedArray implements AutoCloseable {
    private final Resources res;
    private final Resources.Theme theme;
    private final int[] attrs;
    private final String[] values;
    private boolean recycled;

    TypedArray(Resources res, Resources.Theme theme, int[] attrs, String[] values) {
        this.res = res; this.theme = theme; this.attrs = attrs; this.values = values;
    }
    private String v(int i) {
        if (recycled) throw new RuntimeException("Cannot make calls to a recycled instance!");
        return i >= 0 && i < values.length ? values[i] : null;
    }
    /** Reference resolved to its primitive value (colors, dimens, strings), or the file path. */
    private String prim(int i) {
        String v = v(i);
        for (int guard = 0; guard < 8 && v != null && VMValue.kind(v) == 'r'; guard++) {
            String r = res.indroidValue(VMValue.intOf(v, 0));
            if (r == null) return v;
            if (VMValue.kind(r) == 'a' && theme != null) r = theme.resolveRef(r);
            v = r;
        }
        return v;
    }
    public int length() { return values.length; }
    public int getIndexCount() { int n = 0; for (String s : values) if (s != null) n++; return n; }
    public int getIndex(int at) { for (int i = 0, n = 0; i < values.length; i++) if (values[i] != null) { if (n++ == at) return i; } return -1; }
    public Resources getResources() { return res; }
    public CharSequence getText(int i) { String v = prim(i); if (v == null) return null; char k = VMValue.kind(v); return k == 's' ? VMValue.body(v) : k == 'p' ? VMValue.body(v) : VMValue.body(v); }
    public String getString(int i) { CharSequence t = getText(i); return t == null ? null : t.toString(); }
    public String getNonResourceString(int i) { String v = v(i); return v != null && VMValue.kind(v) == 's' ? VMValue.body(v) : null; }
    public CharSequence[] getTextArray(int i) {
        String v = v(i);
        if (v == null || VMValue.kind(v) != 'r') return null;
        try { return res.getTextArray(VMValue.intOf(v, 0)); } catch (RuntimeException e) { return null; }
    }
    public boolean getBoolean(int i, boolean def) { return VMValue.boolOf(prim(i), def); }
    public int getInt(int i, int def) { String v = prim(i); return v == null || VMValue.kind(v) == 'p' ? def : VMValue.intOf(v, def); }
    public int getInteger(int i, int def) { return getInt(i, def); }
    public float getFloat(int i, float def) { return VMValue.floatOf(prim(i), def); }
    public int getColor(int i, int def) {
        String v = prim(i);
        if (v == null) return def;
        if (VMValue.kind(v) == 'p') { ColorStateList c = ColorStateList.indroidInflate(res, VMValue.body(v), theme); return c == null ? def : c.getDefaultColor(); }
        return VMValue.intOf(v, def);
    }
    public ColorStateList getColorStateList(int i) {
        String v = prim(i);
        if (v == null) return null;
        if (VMValue.kind(v) == 'p') return ColorStateList.indroidInflate(res, VMValue.body(v), theme);
        char k = VMValue.kind(v);
        if (k == 'c' || k == 'i') return ColorStateList.valueOf(VMValue.intOf(v, 0));
        return null;
    }
    public float getDimension(int i, float def) { String v = prim(i); return v == null ? def : VMValue.dimension(v, def); }
    public int getDimensionPixelOffset(int i, int def) { String v = prim(i); return v == null ? def : (int) VMValue.dimension(v, def); }
    public int getDimensionPixelSize(int i, int def) {
        String v = prim(i);
        if (v == null) return def;
        float f = VMValue.dimension(v, Float.NaN);
        if (f != f) return def;
        int r = (int) (f >= 0 ? f + 0.5f : f - 0.5f);
        return r != 0 ? r : f == 0 ? 0 : f > 0 ? 1 : -1;
    }
    public int getLayoutDimension(int i, String name) {
        String v = prim(i);
        if (v == null) throw new UnsupportedOperationException("Failed to resolve attribute at index " + i + ": " + name);
        return (int) VMValue.dimension(v, VMValue.intOf(v, -2));
    }
    public int getLayoutDimension(int i, int def) { String v = prim(i); return v == null ? def : (int) VMValue.dimension(v, VMValue.intOf(v, def)); }
    public float getFraction(int i, int base, int pbase, float def) { return VMValue.fraction(prim(i), base, pbase, def); }
    public int getResourceId(int i, int def) { String v = v(i); return v != null && VMValue.kind(v) == 'r' ? VMValue.intOf(v, def) : def; }
    public int getSourceResourceId(int i, int def) { return def; }
    public Drawable getDrawable(int i) {
        String v = v(i);
        if (v == null) return null;
        int id = VMValue.kind(v) == 'r' ? VMValue.intOf(v, 0) : 0;
        String p = prim(i);
        if (p == null) return null;
        return res.drawableFor(id, p, theme);
    }
    public Drawable getDrawableForDensity(int i, int density) { return getDrawable(i); }
    public android.graphics.Typeface getFont(int i) { return v(i) == null ? null : android.graphics.Typeface.DEFAULT; }
    public int getType(int i) {
        String v = v(i);
        if (v == null) return 0;
        TypedValue t = new TypedValue();
        res.fillValue(v, t, true);
        return t.type;
    }
    public boolean hasValue(int i) { return v(i) != null; }
    public boolean hasValueOrEmpty(int i) { return v(i) != null; }
    public TypedValue peekValue(int i) { String v = v(i); if (v == null) return null; TypedValue t = new TypedValue(); res.fillValue(v, t, true); return t; }
    public boolean getValue(int i, TypedValue out) { String v = v(i); if (v == null) return false; res.fillValue(v, out, true); return true; }
    public String getPositionDescription() { return "indroid"; }
    public int getChangingConfigurations() { return 0; }
    public void recycle() { recycled = true; }
    public void close() { recycle(); }
    public String toString() { return java.util.Arrays.toString(values); }
}
