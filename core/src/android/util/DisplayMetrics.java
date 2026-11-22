package android.util;
/** indroid lays out in points: 1 px == 1 dp == 1 iOS point. */
public class DisplayMetrics {
    public static final int DENSITY_DEFAULT = 160;
    public float density = 1f, scaledDensity = 1f, xdpi = 160f, ydpi = 160f;
    public int densityDpi = 160, widthPixels = 390, heightPixels = 844;
    public void setToDefaults() {}
}
