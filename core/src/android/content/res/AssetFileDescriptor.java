package android.content.res;

public class AssetFileDescriptor implements java.io.Closeable {
    public static final long UNKNOWN_LENGTH = -1;
    public long getLength() { return UNKNOWN_LENGTH; }
    public long getStartOffset() { return 0; }
    public long getDeclaredLength() { return UNKNOWN_LENGTH; }
    public java.io.FileInputStream createInputStream() throws java.io.IOException { throw new java.io.IOException("indroid: no asset files"); }
    public void close() {}
}
