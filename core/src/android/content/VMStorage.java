package android.content;
/** indroid: small key/value files in the app's private data directory (host-provided). */
public final class VMStorage {
    private VMStorage() {}
    public static native String load(String path);
    public static native boolean save(String path, String data);
}
