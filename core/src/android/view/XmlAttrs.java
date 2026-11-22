package android.view;

import android.util.AttributeSet;

/**
 * indroid: attributes of one inflated XML element. Values arrive pre-resolved
 * from the VM with a type prefix: s: string, i: int, d: dimension ("16dp"),
 * c: color (int), r: resource reference (int), b: boolean, f: float.
 */
public final class XmlAttrs implements AttributeSet {
    final String[] names;
    final String[] values;
    final int[] ids;
    private final android.content.Context ctx;
    XmlAttrs(String[] n, String[] v) { this(n, v, null); }
    XmlAttrs(String[] n, String[] v, android.content.Context c) {
        // names may carry the attribute's resource id: "textColor\u000516842904"
        ids = new int[n.length];
        for (int i = 0; i < n.length; i++) {
            int q = n[i].indexOf('\u0005');
            if (q >= 0) { try { ids[i] = Integer.parseInt(n[i].substring(q + 1)); } catch (RuntimeException e) {} n[i] = n[i].substring(0, q); }
        }
        names = n; values = v; ctx = c;
    }
    /** indroid: attributes with known resource ids (from a parsed XML file). */
    public static XmlAttrs indroidFrom(String[] n, int[] ids, String[] v, android.content.Context c) {
        String[] names = new String[n.length];
        for (int i = 0; i < n.length; i++) names[i] = ids[i] != 0 ? n[i] + '\u0005' + ids[i] : n[i];
        return new XmlAttrs(names, v.clone(), c);
    }
    public static final XmlAttrs EMPTY = new XmlAttrs(new String[0], new String[0]);
    /** Attributes whose references are identities, not values to resolve. */
    private static final String REF_NAMES = ",id,layout,src,srcCompat,style,theme,background,foreground,drawableLeft,drawableRight,drawableTop,drawableBottom,drawableStart,drawableEnd,button,thumb,track,progressDrawable,divider,listSelector,entries,menu,"
        + "layout_below,layout_above,layout_toLeftOf,layout_toRightOf,layout_toStartOf,layout_toEndOf,layout_alignTop,layout_alignBottom,layout_alignLeft,layout_alignRight,layout_alignStart,layout_alignEnd,layout_alignBaseline,labelFor,";

    int index(String name) { for (int i = 0; i < names.length; i++) if (names[i].equals(name)) return i; return -1; }
    public boolean has(String name) { return index(name) >= 0; }
    /** Encoded value with ?attr resolved through the inflating context's theme. */
    public String raw(String name) { int i = index(name); return i < 0 ? null : resolved(i); }
    /** The value as written in the XML (may be "a:<attr>"). */
    public String indroidRawValue(int i) { return values[i]; }
    String resolved(int i) {
        String v = values[i];
        if (v == null || !v.startsWith("a:") || ctx == null) return v;
        android.content.res.Resources.Theme t = ctx.getTheme();
        String r = t == null ? null : t.indroidAttr(android.content.res.VMValue.intOf(v, 0));
        if (r == null) return null;
        if (r.startsWith("r:") && REF_NAMES.indexOf("," + names[i] + ",") < 0) {
            String p = ctx.getResources().indroidValue(android.content.res.VMValue.intOf(r, 0));
            if (p != null && !p.startsWith("p:") && !p.startsWith("r:")) return p;
        }
        return r;
    }
    public String string(String name) {
        String v = raw(name);
        if (v == null) return null;
        return v.length() >= 2 && v.charAt(1) == ':' ? v.substring(2) : v;
    }
    public int integer(String name, int def) {
        String v = raw(name);
        if (v == null) return def;
        String b = v.substring(2);
        try {
            switch (v.charAt(0)) {
                case 'i': case 'c': case 'r': return Integer.parseInt(b);
                case 'b': return b.equals("true") ? 1 : 0;
                case 'f': return (int) Float.parseFloat(b);
                case 'd': return (int) dimension(name, def);
                default: return Integer.decode(b);
            }
        } catch (RuntimeException e) { return def; }
    }
    /** Dimension in indroid pixels (1px == 1dp == 1sp). match_parent/wrap_content arrive as i:-1 / i:-2. */
    public float dimension(String name, float def) {
        String v = raw(name);
        if (v == null) return def;
        String b = v.substring(2);
        if (v.charAt(0) == 'i') return Integer.parseInt(b);
        int u = b.length();
        while (u > 0 && Character.isLetter(b.charAt(u - 1))) u--;
        try { return Float.parseFloat(b.substring(0, u)); } catch (RuntimeException e) { return def; }
    }
    public boolean bool(String name, boolean def) {
        String v = string(name);
        return v == null ? def : v.equals("true") || v.equals("1");
    }
    public boolean isRef(String name) { String v = raw(name); return v != null && v.startsWith("r:"); }

    public int getAttributeCount() { return names.length; }
    public String getAttributeName(int i) { return names[i]; }
    public String getAttributeValue(int i) { String v = resolved(i); return v == null ? null : v.length() >= 2 ? v.substring(2) : v; }
    public String getAttributeValue(String ns, String name) { return string(name); }
    public String getPositionDescription() { return "indroid layout"; }
    public int getAttributeNameResource(int i) { return ids[i]; }
    public int getAttributeListValue(String ns, String a, String[] opts, int d) {
        String v = string(a); if (v == null) return d;
        for (int i = 0; i < opts.length; i++) if (opts[i].equals(v)) return i;
        return d;
    }
    public boolean getAttributeBooleanValue(String ns, String a, boolean d) { return bool(a, d); }
    public int getAttributeResourceValue(String ns, String a, int d) { return isRef(a) ? integer(a, d) : d; }
    public int getAttributeIntValue(String ns, String a, int d) { return integer(a, d); }
    public int getAttributeUnsignedIntValue(String ns, String a, int d) { return integer(a, d); }
    public float getAttributeFloatValue(String ns, String a, float d) { return dimension(a, d); }
    public int getAttributeResourceValue(int i, int d) { String v = resolved(i); return v != null && v.startsWith("r:") ? Integer.parseInt(v.substring(2)) : d; }
    public int getAttributeIntValue(int i, int d) { return integer(names[i], d); }
    public boolean getAttributeBooleanValue(int i, boolean d) { return bool(names[i], d); }
    public float getAttributeFloatValue(int i, float d) { return dimension(names[i], d); }
    public String getIdAttribute() { return string("id"); }
    public String getClassAttribute() { return string("class"); }
    public int getIdAttributeResourceValue(int d) { return integer("id", d); }
    public int getStyleAttribute() { return integer("style", 0); }
}
