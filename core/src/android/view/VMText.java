package android.view;

/** indroid: text metrics from the host (UIKit on iOS, an approximation headless). */
public final class VMText {
    private VMText() {}
    public static final int BOLD = 1, ITALIC = 2, MONOSPACE = 4;
    /**
     * Measures text laid out in at most {@code maxWidth} points (<= 0: unbounded).
     * flags: bit0 bold, bit1 italic, bit2 monospace, bits 8..15 max lines (0 = unlimited).
     * Returns ceil(width) in the low 32 bits and ceil(height) in the high 32 bits.
     */
    public static native long measure(String text, float size, int flags, float maxWidth);
    public static int width(String text, float size, int flags) { return (int) measure(text, size, flags, 0); }
    public static int widthOf(long m) { return (int) m; }
    public static int heightOf(long m) { return (int) (m >>> 32); }
}
