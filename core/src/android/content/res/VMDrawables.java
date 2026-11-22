package android.content.res;

import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.Drawable;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.InsetDrawable;
import android.graphics.drawable.LayerDrawable;
import android.graphics.drawable.RippleDrawable;
import android.graphics.drawable.StateListDrawable;
import java.util.ArrayList;

/** indroid: inflates drawable XML (shape, selector, layer-list, inset, ripple, color, bitmap, vector). */
final class VMDrawables {
    private VMDrawables() {}
    static final int ATTR_DRAWABLE = 0x01010199, ATTR_COLOR = 0x010101a5, ATTR_SHAPE = 0x010101b5, ATTR_RADIUS = 0x010101a8,
        ATTR_WIDTH = 0x01010159, ATTR_HEIGHT = 0x01010155, ATTR_INSET = 0x010104b5, ATTR_INSET_LEFT = 0x010101b7,
        ATTR_INSET_RIGHT = 0x010101b8, ATTR_INSET_TOP = 0x010101b9, ATTR_INSET_BOTTOM = 0x010101ba, ATTR_START_COLOR = 0x0101019d,
        ATTR_TOP_LEFT_RADIUS = 0x010101ab, ATTR_ID = 0x010100d0, ATTR_LEFT = 0x010101ad, ATTR_TOP = 0x010101ae,
        ATTR_RIGHT = 0x010101af, ATTR_BOTTOM = 0x010101b0, ATTR_SRC = 0x01010119, ATTR_TINT = 0x01010121;

    static Drawable inflate(Resources res, String path, Resources.Theme theme) {
        VMXml root = VMXml.parse(res.indroidXml(path));
        return root == null ? null : inflate(res, root, theme);
    }

    private static String attr(VMXml n, int id, String name, Resources.Theme theme) {
        String v = n.get(id);
        if (v == null) v = n.get(name);
        if (v != null && VMValue.kind(v) == 'a' && theme != null) v = theme.resolveRef(v);
        return v;
    }
    private static String prim(Resources res, String v, Resources.Theme theme) {
        for (int g = 0; g < 8 && v != null && VMValue.kind(v) == 'r'; g++) {
            String r = res.indroidValue(VMValue.intOf(v, 0));
            if (r != null && VMValue.kind(r) == 'a' && theme != null) r = theme.resolveRef(r);
            if (r == null) break;
            v = r;
        }
        return v;
    }
    private static int color(Resources res, String v, Resources.Theme theme, int def) {
        v = prim(res, v, theme);
        if (v == null) return def;
        if (VMValue.kind(v) == 'p') { ColorStateList c = ColorStateList.indroidInflate(res, VMValue.body(v), theme); return c == null ? def : c.getDefaultColor(); }
        return VMValue.intOf(v, def);
    }
    private static float dim(Resources res, String v, Resources.Theme theme, float def) { v = prim(res, v, theme); return v == null ? def : VMValue.dimension(v, def); }
    private static Drawable child(Resources res, VMXml n, Resources.Theme theme) {
        String d = attr(n, ATTR_DRAWABLE, "drawable", theme);
        if (d != null) {
            int id = VMValue.kind(d) == 'r' ? VMValue.intOf(d, 0) : 0;
            String p = prim(res, d, theme);
            if (p != null) return res.drawableFor(id, p, theme);
        }
        return n.children.isEmpty() ? null : inflate(res, n.children.get(0), theme);
    }

    static Drawable inflate(Resources res, VMXml n, Resources.Theme theme) {
        String tag = n.tag;
        if (tag.equals("shape")) {
            GradientDrawable g = new GradientDrawable();
            String s = attr(n, ATTR_SHAPE, "shape", theme);
            int shape = s == null ? 0 : VMValue.intOf(s, 0);
            if (s != null && VMValue.kind(s) == 's') { String b = VMValue.body(s); shape = b.equals("oval") ? 1 : b.equals("line") ? 2 : b.equals("ring") ? 3 : 0; }
            g.setShape(shape);
            for (VMXml c : n.children) {
                if (c.tag.equals("solid")) g.setColor(color(res, attr(c, ATTR_COLOR, "color", theme), theme, 0));
                else if (c.tag.equals("corners")) {
                    float r = dim(res, attr(c, ATTR_RADIUS, "radius", theme), theme, 0);
                    float tl = dim(res, attr(c, ATTR_TOP_LEFT_RADIUS, "topLeftRadius", theme), theme, r);
                    float tr = dim(res, attr(c, 0x010101ac, "topRightRadius", theme), theme, r);
                    float bl = dim(res, attr(c, 0x010101aa, "bottomLeftRadius", theme), theme, r);
                    float br = dim(res, attr(c, 0x010101a9, "bottomRightRadius", theme), theme, r);
                    if (tl == r && tr == r && bl == r && br == r) g.setCornerRadius(r);
                    else g.setCornerRadii(new float[] { tl, tl, tr, tr, br, br, bl, bl });
                } else if (c.tag.equals("stroke")) {
                    g.setStroke((int) dim(res, attr(c, ATTR_WIDTH, "width", theme), theme, 0), color(res, attr(c, ATTR_COLOR, "color", theme), theme, 0));
                } else if (c.tag.equals("size")) {
                    g.setSize((int) dim(res, attr(c, ATTR_WIDTH, "width", theme), theme, -1), (int) dim(res, attr(c, ATTR_HEIGHT, "height", theme), theme, -1));
                } else if (c.tag.equals("gradient")) {
                    int start = color(res, attr(c, ATTR_START_COLOR, "startColor", theme), theme, 0);
                    int end = color(res, attr(c, 0x0101019e, "endColor", theme), theme, start);
                    g.setColors(new int[] { start, end });
                }
            }
            return g;
        }
        if (tag.equals("selector") || tag.equals("animated-selector")) {
            StateListDrawable sl = new StateListDrawable();
            for (VMXml item : n.children) {
                if (!item.tag.equals("item")) continue;
                Drawable d = child(res, item, theme);
                if (d == null) continue;
                ArrayList<Integer> st = new ArrayList<Integer>();
                for (int i = 0; i < item.names.length; i++) {
                    if (!item.names[i].startsWith("state_")) continue;
                    int id = item.ids[i] != 0 ? item.ids[i] : VMValue.stateId(item.names[i]);
                    if (id != 0) st.add(Integer.valueOf(VMValue.boolOf(item.values[i], true) ? id : -id));
                }
                int[] s = new int[st.size()];
                for (int i = 0; i < s.length; i++) s[i] = st.get(i).intValue();
                sl.addState(s, d);
            }
            return sl;
        }
        if (tag.equals("layer-list") || tag.equals("transition")) {
            ArrayList<Drawable> layers = new ArrayList<Drawable>();
            ArrayList<int[]> insets = new ArrayList<int[]>();
            for (VMXml item : n.children) {
                if (!item.tag.equals("item")) continue;
                Drawable d = child(res, item, theme);
                if (d == null) continue;
                layers.add(d);
                insets.add(new int[] { (int) dim(res, attr(item, ATTR_LEFT, "left", theme), theme, 0), (int) dim(res, attr(item, ATTR_TOP, "top", theme), theme, 0),
                    (int) dim(res, attr(item, ATTR_RIGHT, "right", theme), theme, 0), (int) dim(res, attr(item, ATTR_BOTTOM, "bottom", theme), theme, 0) });
            }
            LayerDrawable ld = new LayerDrawable(layers.toArray(new Drawable[0]));
            for (int i = 0; i < insets.size(); i++) { int[] in = insets.get(i); ld.setLayerInsetLeft(i, in[0]); ld.setLayerInsetTop(i, in[1]); ld.setLayerInsetRight(i, in[2]); ld.setLayerInsetBottom(i, in[3]); }
            return ld;
        }
        if (tag.equals("inset")) {
            Drawable d = child(res, n, theme);
            int all = (int) dim(res, attr(n, ATTR_INSET, "inset", theme), theme, 0);
            return new InsetDrawable(d,
                (int) dim(res, attr(n, ATTR_INSET_LEFT, "insetLeft", theme), theme, all), (int) dim(res, attr(n, ATTR_INSET_TOP, "insetTop", theme), theme, all),
                (int) dim(res, attr(n, ATTR_INSET_RIGHT, "insetRight", theme), theme, all), (int) dim(res, attr(n, ATTR_INSET_BOTTOM, "insetBottom", theme), theme, all));
        }
        if (tag.equals("ripple")) {
            int rc = color(res, attr(n, ATTR_COLOR, "color", theme), theme, 0x1f000000);
            Drawable content = null, mask = null;
            for (VMXml item : n.children) {
                if (!item.tag.equals("item")) continue;
                String id = attr(item, ATTR_ID, "id", theme);
                Drawable d = child(res, item, theme);
                if (id != null && VMValue.intOf(id, 0) == 0x0102002e) mask = d; else if (content == null) content = d;
            }
            return new RippleDrawable(ColorStateList.valueOf(rc), content, mask);
        }
        if (tag.equals("color")) return new ColorDrawable(color(res, attr(n, ATTR_COLOR, "color", theme), theme, 0));
        if (tag.equals("bitmap") || tag.equals("nine-patch")) {
            String src = attr(n, ATTR_SRC, "src", theme);
            if (src != null && VMValue.kind(src) == 'r') return Drawable.fromResource(VMValue.intOf(src, 0));
            return null;
        }
        if (tag.equals("vector")) return android.graphics.drawable.VectorDrawable.indroidInflate(n, res, theme);
        if (tag.equals("animated-vector")) {
            android.graphics.drawable.AnimatedVectorDrawable v = new android.graphics.drawable.AnimatedVectorDrawable();
            String ref = attr(n, ATTR_DRAWABLE, "drawable", theme);
            if (ref != null && VMValue.kind(ref) == 'r') v.setVector(res.getDrawable(VMValue.intOf(ref, 0), theme));
            return v;
        }
        return null;
    }
}
