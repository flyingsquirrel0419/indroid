package android.view;
/** indroid: package-private view lookups for the host bridge. */
public final class VMViews {
    private VMViews() {}
    public static native boolean overridesOnDraw(View view);
    public static native boolean overridesOnTouch(View view);
    public static View byUid(View root, int uid) { return root.findByUid(uid); }
}
