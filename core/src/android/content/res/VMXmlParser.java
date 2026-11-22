package android.content.res;

import java.util.ArrayList;
import org.xmlpull.v1.XmlPullParser;
import org.xmlpull.v1.XmlPullParserException;

/** XmlResourceParser over a VM-serialized binary XML file. Attribute values resolve through the theme of the given context. */
public final class VMXmlParser implements XmlResourceParser, XmlPullParser {
    private static final String ANDROID_NS = "http://schemas.android.com/apk/res/android";
    private final ArrayList<Object[]> events = new ArrayList<Object[]>(); // {type, VMXml}
    private int pos = -1;
    private final android.content.Context ctx;

    public VMXmlParser(String serialized, android.content.Context ctx) {
        this.ctx = ctx;
        events.add(new Object[] { START_DOCUMENT, null, 0 });
        VMXml root = VMXml.parse(serialized);
        if (root != null) flatten(root, 1);
        events.add(new Object[] { END_DOCUMENT, null, 0 });
    }
    private void flatten(VMXml n, int depth) {
        events.add(new Object[] { START_TAG, n, depth });
        for (VMXml c : n.children) flatten(c, depth + 1);
        events.add(new Object[] { END_TAG, n, depth });
    }
    private VMXml node() { return pos < 0 ? null : (VMXml) events.get(pos)[1]; }
    private android.view.XmlAttrs attrs;
    private int attrsPos = -2;
    /** indroid: the current element's attributes as XmlAttrs (for obtainStyledAttributes). */
    public android.view.XmlAttrs indroidAttrs() {
        if (attrsPos != pos) {
            VMXml n = node();
            attrs = n == null ? android.view.XmlAttrs.EMPTY : android.view.XmlAttrs.indroidFrom(n.names, n.ids, n.values, ctx);
            attrsPos = pos;
        }
        return attrs;
    }

    public int getEventType() { return pos < 0 ? START_DOCUMENT : ((Integer) events.get(pos)[0]).intValue(); }
    public int next() throws XmlPullParserException { if (pos < events.size() - 1) pos++; return getEventType(); }
    public int nextToken() throws XmlPullParserException { return next(); }
    public int nextTag() throws XmlPullParserException {
        int t = next();
        if (t != START_TAG && t != END_TAG) throw new XmlPullParserException("expected start or end tag");
        return t;
    }
    public String nextText() throws XmlPullParserException { next(); return ""; }
    public void require(int type, String ns, String name) throws XmlPullParserException {
        if (type != getEventType() || (name != null && !name.equals(getName()))) throw new XmlPullParserException("expected " + TYPES[type] + " " + name);
    }
    public int getDepth() { return pos < 0 ? 0 : ((Integer) events.get(pos)[2]).intValue() - (getEventType() == END_TAG ? 0 : 0); }
    public String getName() { VMXml n = node(); return n == null ? null : n.tag; }
    public String getNamespace() { return ""; }
    public String getPrefix() { return null; }
    public boolean isEmptyElementTag() { VMXml n = node(); return n != null && n.children.isEmpty(); }
    public String getText() { return null; }
    public char[] getTextCharacters(int[] holder) { return null; }
    public boolean isWhitespace() { return true; }
    public int getLineNumber() { return pos; }
    public int getColumnNumber() { return 0; }
    public String getPositionDescription() { return "Binary XML file line #" + pos; }
    public void setFeature(String n, boolean s) {}
    public boolean getFeature(String n) { return false; }
    public void setProperty(String n, Object v) {}
    public Object getProperty(String n) { return null; }
    public void setInput(java.io.Reader r) {}
    public void setInput(java.io.InputStream s, String enc) {}
    public String getInputEncoding() { return "UTF-8"; }
    public void defineEntityReplacementText(String n, String r) {}
    public int getNamespaceCount(int depth) { return 0; }
    public String getNamespacePrefix(int p) { return null; }
    public String getNamespaceUri(int p) { return null; }
    public String getNamespace(String prefix) { return null; }
    public int getAttributeCount() { VMXml n = node(); return getEventType() == START_TAG && n != null ? n.names.length : -1; }
    public String getAttributeNamespace(int i) { VMXml n = node(); return n != null && n.ids[i] != 0 && (n.ids[i] >>> 24) == 1 ? ANDROID_NS : ""; }
    public String getAttributeName(int i) { return node().names[i]; }
    public String getAttributePrefix(int i) { return null; }
    public String getAttributeType(int i) { return "CDATA"; }
    public boolean isAttributeDefault(int i) { return false; }
    public String getAttributeValue(int i) { return indroidAttrs().getAttributeValue(i); }
    public String getAttributeValue(String ns, String name) { return indroidAttrs().getAttributeValue(ns, name); }
    // AttributeSet
    public int getAttributeNameResource(int i) { return node().ids[i]; }
    public int getAttributeListValue(String ns, String a, String[] opts, int d) { return indroidAttrs().getAttributeListValue(ns, a, opts, d); }
    public boolean getAttributeBooleanValue(String ns, String a, boolean d) { return indroidAttrs().getAttributeBooleanValue(ns, a, d); }
    public int getAttributeResourceValue(String ns, String a, int d) { return indroidAttrs().getAttributeResourceValue(ns, a, d); }
    public int getAttributeIntValue(String ns, String a, int d) { return indroidAttrs().getAttributeIntValue(ns, a, d); }
    public int getAttributeUnsignedIntValue(String ns, String a, int d) { return indroidAttrs().getAttributeUnsignedIntValue(ns, a, d); }
    public float getAttributeFloatValue(String ns, String a, float d) { return indroidAttrs().getAttributeFloatValue(ns, a, d); }
    public int getAttributeListValue(int i, String[] opts, int d) { String v = getAttributeValue(i); if (v == null) return d; for (int k = 0; k < opts.length; k++) if (opts[k].equals(v)) return k; return d; }
    public boolean getAttributeBooleanValue(int i, boolean d) { return indroidAttrs().getAttributeBooleanValue(i, d); }
    public int getAttributeResourceValue(int i, int d) { return indroidAttrs().getAttributeResourceValue(i, d); }
    public int getAttributeIntValue(int i, int d) { return indroidAttrs().getAttributeIntValue(i, d); }
    public int getAttributeUnsignedIntValue(int i, int d) { return indroidAttrs().getAttributeIntValue(i, d); }
    public float getAttributeFloatValue(int i, float d) { return indroidAttrs().getAttributeFloatValue(i, d); }
    public String getIdAttribute() { return indroidAttrs().getIdAttribute(); }
    public String getClassAttribute() { return indroidAttrs().getClassAttribute(); }
    public int getIdAttributeResourceValue(int d) { return indroidAttrs().getIdAttributeResourceValue(d); }
    public int getStyleAttribute() { return indroidAttrs().getStyleAttribute(); }
    public void close() {}
}
