package android.content.res;

import android.os.Parcel;
import android.os.Parcelable;
import java.util.Arrays;

public class ColorStateList implements Parcelable {
    private final int[][] stateSpecs;
    private final int[] colors;
    private final int defaultColor;

    public ColorStateList(int[][] states, int[] colors) {
        this.stateSpecs = states;
        this.colors = colors;
        int def = colors.length > 0 ? colors[0] : 0;
        for (int i = 0; i < states.length; i++) if (states[i].length == 0) { def = colors[i]; break; }
        defaultColor = def;
    }
    public static ColorStateList valueOf(int c) { return new ColorStateList(new int[][] { new int[0] }, new int[] { c }); }
    public int getDefaultColor() { return defaultColor; }
    public int getColorForState(int[] stateSet, int def) {
        for (int i = 0; i < stateSpecs.length; i++) if (VMValue.stateMatches(stateSpecs[i], stateSet)) return colors[i];
        return def;
    }
    public boolean isStateful() { return stateSpecs.length > 1 || (stateSpecs.length == 1 && stateSpecs[0].length > 0); }
    public boolean isOpaque() { for (int c : colors) if ((c >>> 24) != 0xff) return false; return true; }
    public ColorStateList withAlpha(int alpha) {
        int[] c = colors.clone();
        for (int i = 0; i < c.length; i++) c[i] = (c[i] & 0xffffff) | (alpha << 24);
        return new ColorStateList(stateSpecs, c);
    }
    public ColorStateList withLStar(float l) { return this; }
    public int getChangingConfigurations() { return 0; }
    public int[][] getStates() { return stateSpecs; }
    public int[] getColors() { return colors; }
    public boolean hasState(int state) { for (int[] s : stateSpecs) for (int x : s) if (x == state || x == -state) return true; return false; }
    public String toString() { return "ColorStateList{mStateSpecs=" + Arrays.deepToString(stateSpecs) + "mColors=" + Arrays.toString(colors) + "mDefaultColor=" + defaultColor + '}'; }
    public int describeContents() { return 0; }
    public void writeToParcel(Parcel dest, int flags) {}
    public static final Parcelable.Creator<ColorStateList> CREATOR = new Parcelable.Creator<ColorStateList>() {
        public ColorStateList createFromParcel(Parcel p) { return valueOf(0); }
        public ColorStateList[] newArray(int n) { return new ColorStateList[n]; }
    };

    static final int ATTR_COLOR = 0x010101a5, ATTR_ALPHA = 0x0101031f, ATTR_LSTAR = 0x01010647;
    /** indroid: inflates res/color/*.xml (a <selector> of <item android:color android:state_*>). */
    public static ColorStateList indroidInflate(Resources res, String path, Resources.Theme theme) {
        if (path.startsWith("res/framework/")) return null;
        VMXml root = VMXml.parse(res.indroidXml(path));
        if (root == null) return null;
        int n = root.children.size();
        int[][] states = new int[n][];
        int[] cols = new int[n];
        int k = 0;
        for (VMXml item : root.children) {
            String c = item.get(ATTR_COLOR);
            if (c == null) c = item.get("color");
            if (c != null && VMValue.kind(c) == 'a' && theme != null) c = theme.resolveRef(c);
            int color = 0;
            if (c != null) {
                if (VMValue.kind(c) == 'r') {
                    String r = res.indroidValue(VMValue.intOf(c, 0));
                    if (r != null && VMValue.kind(r) == 'p') { ColorStateList nested = indroidInflate(res, VMValue.body(r), theme); color = nested == null ? 0 : nested.getDefaultColor(); }
                    else color = VMValue.intOf(r, 0);
                } else color = VMValue.intOf(c, 0);
            }
            String a = item.get(ATTR_ALPHA);
            if (a == null) a = item.get("alpha");
            if (a != null) {
                if (VMValue.kind(a) == 'a' && theme != null) a = theme.resolveRef(a);
                if (a != null && VMValue.kind(a) == 'r') a = res.indroidValue(VMValue.intOf(a, 0));
                float f = VMValue.floatOf(a, 1f);
                int alpha = Math.round(((color >>> 24) & 0xff) * Math.max(0f, Math.min(1f, f)));
                color = (color & 0xffffff) | (alpha << 24);
            }
            java.util.ArrayList<Integer> st = new java.util.ArrayList<Integer>();
            for (int i = 0; i < item.names.length; i++) {
                if (!item.names[i].startsWith("state_")) continue;
                int id = item.ids[i] != 0 ? item.ids[i] : VMValue.stateId(item.names[i]);
                if (id == 0) continue;
                st.add(Integer.valueOf(VMValue.boolOf(item.values[i], true) ? id : -id));
            }
            int[] s = new int[st.size()];
            for (int i = 0; i < s.length; i++) s[i] = st.get(i).intValue();
            states[k] = s; cols[k] = color; k++;
        }
        return new ColorStateList(states, cols);
    }
}
