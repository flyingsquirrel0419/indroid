package android.view;

import android.graphics.Rect;

public class Gravity {
    public static final int NO_GRAVITY = 0, AXIS_SPECIFIED = 1, AXIS_PULL_BEFORE = 2, AXIS_PULL_AFTER = 4, AXIS_CLIP = 8,
        AXIS_X_SHIFT = 0, AXIS_Y_SHIFT = 4,
        LEFT = 3, RIGHT = 5, TOP = 48, BOTTOM = 80, CENTER_VERTICAL = 16, CENTER_HORIZONTAL = 1,
        CENTER = 17, FILL = 0x77, FILL_HORIZONTAL = 7, FILL_VERTICAL = 0x70, CLIP_VERTICAL = 0x80, CLIP_HORIZONTAL = 8,
        HORIZONTAL_GRAVITY_MASK = 7, VERTICAL_GRAVITY_MASK = 0x70, DISPLAY_CLIP_VERTICAL = 0x10000000, DISPLAY_CLIP_HORIZONTAL = 0x01000000,
        RELATIVE_LAYOUT_DIRECTION = 0x00800000, START = 0x00800003, END = 0x00800005, RELATIVE_HORIZONTAL_GRAVITY_MASK = 0x00800007;

    public static int getAbsoluteGravity(int gravity, int layoutDirection) {
        int result = gravity;
        if ((result & RELATIVE_LAYOUT_DIRECTION) > 0) {
            boolean rtl = layoutDirection == 1;
            if ((result & START) == START) { result &= ~START; result |= rtl ? RIGHT : LEFT; }
            else if ((result & END) == END) { result &= ~END; result |= rtl ? LEFT : RIGHT; }
            result &= ~RELATIVE_LAYOUT_DIRECTION;
        }
        return result;
    }
    public static boolean isVertical(int g) { return g > 0 && (g & VERTICAL_GRAVITY_MASK) != 0; }
    public static boolean isHorizontal(int g) { return g > 0 && (g & RELATIVE_HORIZONTAL_GRAVITY_MASK) != 0; }
    public static void apply(int gravity, int w, int h, Rect container, Rect outRect) { apply(gravity, w, h, container, 0, 0, outRect); }
    public static void apply(int gravity, int w, int h, Rect container, Rect outRect, int layoutDirection) { apply(getAbsoluteGravity(gravity, layoutDirection), w, h, container, 0, 0, outRect); }
    public static void apply(int gravity, int w, int h, Rect container, int xAdj, int yAdj, Rect outRect, int layoutDirection) { apply(getAbsoluteGravity(gravity, layoutDirection), w, h, container, xAdj, yAdj, outRect); }
    public static void apply(int gravity, int w, int h, Rect c, int xAdj, int yAdj, Rect out) {
        gravity = getAbsoluteGravity(gravity, 0);
        switch (gravity & HORIZONTAL_GRAVITY_MASK) {
            case CENTER_HORIZONTAL: out.left = c.left + (c.right - c.left - w) / 2 + xAdj; out.right = out.left + w; break;
            case RIGHT: out.right = c.right - xAdj; out.left = out.right - w; break;
            case LEFT: out.left = c.left + xAdj; out.right = out.left + w; break;
            default: out.left = c.left + xAdj; out.right = c.right + xAdj;
        }
        switch (gravity & VERTICAL_GRAVITY_MASK) {
            case CENTER_VERTICAL: out.top = c.top + (c.bottom - c.top - h) / 2 + yAdj; out.bottom = out.top + h; break;
            case BOTTOM: out.bottom = c.bottom - yAdj; out.top = out.bottom - h; break;
            case TOP: out.top = c.top + yAdj; out.bottom = out.top + h; break;
            default: out.top = c.top + yAdj; out.bottom = c.bottom + yAdj;
        }
    }
    public static void applyDisplay(int gravity, Rect display, Rect inoutObj) {}
    public static String toString(int g) { String d = describe(g); return d == null ? "NO GRAVITY" : d.toUpperCase(); }

    /** indroid: compact host description such as "center" or "left|top". */
    public static String describe(int g) {
        if (g == 0 || g == -1) return null;
        g = getAbsoluteGravity(g, 0);
        int h = g & 7, v = g & 0x70;
        if (h == 1 && v == 0x10) return "center";
        StringBuilder sb = new StringBuilder();
        if (h == 1) sb.append("center_horizontal"); else if (h == 5) sb.append("right"); else if (h == 3) sb.append("left");
        if (v == 0x10) sb.append(sb.length() > 0 ? "|" : "").append("center_vertical");
        else if (v == 0x50) sb.append(sb.length() > 0 ? "|" : "").append("bottom");
        else if (v == 0x30) sb.append(sb.length() > 0 ? "|" : "").append("top");
        return sb.length() == 0 ? null : sb.toString();
    }
    static int parse(String s) {
        int g = 0;
        for (String p : s.split("\\|")) {
            p = p.trim();
            if (p.equals("center")) g |= CENTER; else if (p.equals("center_horizontal")) g |= CENTER_HORIZONTAL;
            else if (p.equals("center_vertical")) g |= CENTER_VERTICAL; else if (p.equals("left")) g |= LEFT; else if (p.equals("start")) g |= START;
            else if (p.equals("right")) g |= RIGHT; else if (p.equals("end")) g |= END; else if (p.equals("top")) g |= TOP; else if (p.equals("bottom")) g |= BOTTOM;
            else if (p.equals("fill")) g |= FILL; else if (p.equals("fill_horizontal")) g |= FILL_HORIZONTAL; else if (p.equals("fill_vertical")) g |= FILL_VERTICAL;
        }
        return g;
    }
}
