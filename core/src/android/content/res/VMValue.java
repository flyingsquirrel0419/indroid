package android.content.res;

/**
 * indroid: decoding of the VM's encoded resource values.
 * Prefixes: s: string, i: int, c: color, d: dimension ("16dp"), f: float,
 * b: boolean, r: reference (resource id), a: theme attribute (?attr),
 * p: file path inside the APK (drawables, color state lists, xml).
 */
public final class VMValue {
    private VMValue() {}
    public static char kind(String v) { return v == null || v.length() < 2 || v.charAt(1) != ':' ? 's' : v.charAt(0); }
    public static String body(String v) { return v == null ? null : v.length() >= 2 && v.charAt(1) == ':' ? v.substring(2) : v; }
    public static int intOf(String v, int def) {
        if (v == null) return def;
        String b = body(v);
        try {
            switch (kind(v)) {
                case 'i': case 'c': case 'r': case 'a': return Integer.parseInt(b);
                case 'b': return b.equals("true") ? 1 : 0;
                case 'f': return (int) Float.parseFloat(b);
                case 'd': return (int) dimension(v, def);
                default: return Integer.decode(b);
            }
        } catch (RuntimeException e) { return def; }
    }
    public static float floatOf(String v, float def) {
        if (v == null) return def;
        try {
            switch (kind(v)) {
                case 'f': return Float.parseFloat(body(v));
                case 'q': { String b = body(v); return Float.parseFloat(b.endsWith("p") ? b.substring(0, b.length() - 1) : b); }
                case 'd': return dimension(v, def);
                case 'i': return Integer.parseInt(body(v));
                case 's': return Float.parseFloat(body(v));
                default: return def;
            }
        } catch (RuntimeException e) { return def; }
    }
    /** Dimension in indroid pixels (1px == 1dp == 1sp). */
    public static float dimension(String v, float def) {
        if (v == null) return def;
        String b = body(v);
        char k = kind(v);
        if (k == 'i') { try { return Integer.parseInt(b); } catch (RuntimeException e) { return def; } }
        if (k != 'd' && k != 'f') return def;
        int u = b.length();
        while (u > 0 && Character.isLetter(b.charAt(u - 1))) u--;
        try {
            float f = Float.parseFloat(b.substring(0, u));
            String unit = b.substring(u);
            if (unit.equals("pt")) return f * 160f / 72f;
            if (unit.equals("in")) return f * 160f;
            if (unit.equals("mm")) return f * 160f / 25.4f;
            return f;
        } catch (RuntimeException e) { return def; }
    }
    /** Fraction resource ("q:0.5" or "q:0.5p" for %p) scaled by base / parent base. */
    public static float fraction(String v, float base, float pbase, float def) {
        if (v == null) return def;
        if (kind(v) != 'q') return floatOf(v, def) * base;
        String b = body(v);
        boolean parent = b.endsWith("p");
        try { return Float.parseFloat(parent ? b.substring(0, b.length() - 1) : b) * (parent ? pbase : base); } catch (RuntimeException e) { return def; }
    }
    public static boolean boolOf(String v, boolean def) {
        if (v == null) return def;
        String b = body(v);
        if (kind(v) == 'i') return !b.equals("0");
        return b.equals("true") || b.equals("1");
    }

    /** android.R.attr ids of view states. */
    public static int stateId(String name) {
        if (name.equals("state_pressed")) return 0x010100a7;
        if (name.equals("state_focused")) return 0x0101009c;
        if (name.equals("state_selected")) return 0x010100a1;
        if (name.equals("state_checkable")) return 0x0101009f;
        if (name.equals("state_checked")) return 0x010100a0;
        if (name.equals("state_enabled")) return 0x0101009e;
        if (name.equals("state_window_focused")) return 0x0101009d;
        if (name.equals("state_activated")) return 0x010102fe;
        if (name.equals("state_hovered")) return 0x01010367;
        if (name.equals("state_active")) return 0x010100a2;
        return 0;
    }
    /** StateSet.stateSetMatches semantics: positive ids required, negative ids forbidden. */
    public static boolean stateMatches(int[] spec, int[] state) {
        if (state == null) state = new int[0];
        for (int s : spec) {
            if (s == 0) return true;
            boolean found = false;
            int want = s > 0 ? s : -s;
            for (int x : state) if (x == want) { found = true; break; }
            if (s > 0 ? !found : found) return false;
        }
        return true;
    }
}
