package android.widget;
import android.content.Context;
import android.util.AttributeSet;
public class CheckBox extends CompoundButton {
    public CheckBox(Context c) { this(c, null); }
    public CheckBox(Context c, AttributeSet a) { this(c, a, 16842860); }
    public CheckBox(Context c, AttributeSet a, int s) { super(c, a, s); }
    protected String indroidType() { return "CheckBox"; }
    public CheckBox(Context c, AttributeSet a, int s, int r) { this(c,a,s); }
 public void setStateDescription(CharSequence description) {}
}
