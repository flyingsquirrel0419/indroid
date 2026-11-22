package android.view;
/** indroid natives used by views. */
final class VMBridge {
    private VMBridge() {}
    /** android:onClick="method" -> calls context.method(View). */
    static native void invokeOnClick(Object target, String method, View v);
}
