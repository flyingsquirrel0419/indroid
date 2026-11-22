package android.content.res;

/** indroid: transparent stand-in for a framework drawable that is not bundled (24dp). */
public final class VMPlaceholderDrawable extends android.graphics.drawable.ColorDrawable {
    public VMPlaceholderDrawable() { super(0); }
    public int getIntrinsicWidth() { return 24; }
    public int getIntrinsicHeight() { return 24; }
}
