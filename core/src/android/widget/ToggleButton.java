package android.widget;
import android.content.Context;
import android.util.AttributeSet;
public class ToggleButton extends CompoundButton {
    public ToggleButton(Context c) { super(c); }
    public ToggleButton(Context c, AttributeSet a) { super(c, a); }
    public ToggleButton(Context c, AttributeSet a, int s) { super(c, a, s); }
    protected String indroidType() { return "Switch"; }
    public void setTextOn(CharSequence t) {}
    public void setTextOff(CharSequence t) {}
    public ToggleButton(Context c, AttributeSet a, int s, int r) { this(c,a,s); }
}
