package android.view;
/** indroid: tiny JSON writer for the host UI tree. */
public final class Json {
    private Json() {}
    public static void str(StringBuilder sb, CharSequence s) {
        if (s == null) { sb.append("null"); return; }
        sb.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"': sb.append("\\\""); break;
                case '\\': sb.append("\\\\"); break;
                case '\n': sb.append("\\n"); break;
                case '\r': sb.append("\\r"); break;
                case '\t': sb.append("\\t"); break;
                default:
                    if (c < 0x20) { sb.append("\\u00"); sb.append(Character.forDigit(c >> 4, 16)); sb.append(Character.forDigit(c & 15, 16)); }
                    else sb.append(c);
            }
        }
        sb.append('"');
    }
    public static void key(StringBuilder sb, String k) { sb.append(','); str(sb, k); sb.append(':'); }
    public static void num(StringBuilder sb, float f) { if (f == (int) f) sb.append((int) f); else sb.append(f); }
}
