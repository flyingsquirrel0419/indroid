package android.widget;

/** indroid: intrinsic size (dp) of a drawable resource, from the APK's bitmap headers. */
final class VMImage {
    private VMImage() {}
    /** width in the low 32 bits, height in the high 32 bits; 0 when unknown. */
    static native long size(int resId);
}
