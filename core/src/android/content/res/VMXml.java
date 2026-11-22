package android.content.res;

import java.util.ArrayList;

/** indroid: parsed form of a VM-serialized binary XML file (\u0001tag \u0002name[\u0005id]\u0004value ... \u0003). */
public final class VMXml {
    public final String tag;
    public final String[] names, values;
    public final int[] ids;
    public final ArrayList<VMXml> children = new ArrayList<VMXml>();
    private VMXml(String tag, String[] n, int[] i, String[] v) { this.tag = tag; names = n; ids = i; values = v; }

    public static VMXml parse(String s) {
        if (s == null || s.length() == 0) return null;
        int[] pos = { 0 };
        try { return parse(s, pos); } catch (RuntimeException e) { return null; }
    }
    private static VMXml parse(String s, int[] pos) {
        int p = pos[0];
        if (s.charAt(p) != '\u0001') throw new IllegalStateException("bad xml");
        int e = p + 1;
        while (e < s.length() && s.charAt(e) > '\u0005') e++;
        String tag = s.substring(p + 1, e);
        p = e;
        ArrayList<String> n = new ArrayList<String>(), v = new ArrayList<String>();
        ArrayList<Integer> ids = new ArrayList<Integer>();
        while (p < s.length() && s.charAt(p) == '\u0002') {
            int sep = s.indexOf('\u0004', p);
            String name = s.substring(p + 1, sep);
            int id = 0;
            int q = name.indexOf('\u0005');
            if (q >= 0) { try { id = Integer.parseInt(name.substring(q + 1)); } catch (RuntimeException x) {} name = name.substring(0, q); }
            int end = sep + 1;
            while (end < s.length() && s.charAt(end) > '\u0005') end++;
            n.add(name); ids.add(Integer.valueOf(id)); v.add(s.substring(sep + 1, end));
            p = end;
        }
        int[] ia = new int[ids.size()];
        for (int i = 0; i < ia.length; i++) ia[i] = ids.get(i).intValue();
        VMXml node = new VMXml(tag, n.toArray(new String[0]), ia, v.toArray(new String[0]));
        pos[0] = p;
        while (pos[0] < s.length() && s.charAt(pos[0]) == '\u0001') node.children.add(parse(s, pos));
        if (pos[0] < s.length() && s.charAt(pos[0]) == '\u0003') pos[0]++;
        return node;
    }
    public String get(String name) { for (int i = 0; i < names.length; i++) if (names[i].equals(name)) return values[i]; return null; }
    public String get(int attrId) { for (int i = 0; i < ids.length; i++) if (ids[i] == attrId) return values[i]; return null; }
}
