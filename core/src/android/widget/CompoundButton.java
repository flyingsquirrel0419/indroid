package android.widget;

import android.content.Context;
import android.graphics.drawable.Drawable;
import android.util.AttributeSet;
import android.view.Json;

public abstract class CompoundButton extends Button implements Checkable {
    public interface OnCheckedChangeListener { void onCheckedChanged(CompoundButton b, boolean checked); }
    private static final int[] CHECKED_STATE_SET = { 0x010100a0 };
    static final int[] ATTRS = { 16843015 /*button*/, 16843014 /*checked*/, 16843887 /*buttonTint*/ };
    private boolean checked, broadcasting;
    private OnCheckedChangeListener listener, internalListener;
    private Drawable buttonDrawable;
    private android.content.res.ColorStateList buttonTint;
    private android.graphics.PorterDuff.Mode buttonMode;

    public CompoundButton(Context c) { this(c, null); }
    public CompoundButton(Context c, AttributeSet a) { this(c, a, 0); }
    public CompoundButton(Context c, AttributeSet a, int s) { this(c, a, s, 0); }
    public CompoundButton(Context c, AttributeSet a, int s, int r) {
        super(c, a, s, r);
        if (c == null) return;
        android.content.res.TypedArray t = c.obtainStyledAttributes(a, ATTRS, s, r);
        if (t.hasValue(0)) { try { setButtonDrawable(t.getDrawable(0)); } catch (RuntimeException e) {} }
        if (t.hasValue(2)) { android.content.res.ColorStateList tint = t.getColorStateList(2); if (tint != null) setButtonTintList(tint); }
        checked = t.getBoolean(1, false);
        t.recycle();
        refreshDrawableState();
    }
    public boolean isChecked() { return checked; }
    public void setChecked(boolean c) {
        if (checked == c) return;
        checked = c;
        refreshDrawableState();
        invalidate();
        if (broadcasting) return;
        broadcasting = true;
        if (listener != null) listener.onCheckedChanged(this, c);
        if (internalListener != null) internalListener.onCheckedChanged(this, c);
        broadcasting = false;
        if (getParent() instanceof RadioGroup && c) ((RadioGroup) getParent()).childChecked(this);
    }
    public void toggle() { setChecked(!checked); }
    public boolean performClick() { toggle(); return super.performClick(); }
    public void setOnCheckedChangeListener(OnCheckedChangeListener l) { listener = l; }
    void setOnCheckedChangeWidgetListener(OnCheckedChangeListener l) { internalListener = l; }
    public void setButtonDrawable(int r) { setButtonDrawable(r == 0 ? null : getContext().getDrawable(r)); }
    public void setButtonDrawable(Drawable d) {
        if (d == buttonDrawable) return;
        if (buttonDrawable != null) buttonDrawable.setCallback(null);
        buttonDrawable = d;
        if (d != null) {
            d.setCallback(this);
            if (d.isStateful()) d.setState(getDrawableState());
            if (buttonTint != null) { buttonDrawable = d = d.mutate(); d.setTintList(buttonTint); }
        }
        requestLayout();
        invalidate();
    }
    public Drawable getButtonDrawable() { return buttonDrawable; }
    public void setButtonTintList(android.content.res.ColorStateList c) { buttonTint = c; if (buttonDrawable != null) { buttonDrawable = buttonDrawable.mutate(); buttonDrawable.setTintList(c); } invalidate(); }
    public android.content.res.ColorStateList getButtonTintList() { return buttonTint; }
    public void setButtonTintMode(android.graphics.PorterDuff.Mode m) { buttonMode = m; if (buttonDrawable != null) buttonDrawable.setTintMode(m); }
    public android.graphics.PorterDuff.Mode getButtonTintMode() { return buttonMode; }
    public void setButtonIcon(android.graphics.drawable.Icon icon) {}
    protected int[] onCreateDrawableState(int extraSpace) {
        int[] s = super.onCreateDrawableState(extraSpace + 1);
        if (checked) mergeDrawableStates(s, CHECKED_STATE_SET);
        return s;
    }
    protected void drawableStateChanged() {
        super.drawableStateChanged();
        if (buttonDrawable != null && buttonDrawable.isStateful() && buttonDrawable.setState(getDrawableState())) invalidate();
    }
    protected boolean verifyDrawable(Drawable who) { return super.verifyDrawable(who) || who == buttonDrawable; }
    public void jumpDrawablesToCurrentState() { super.jumpDrawablesToCurrentState(); if (buttonDrawable != null) buttonDrawable.jumpToCurrentState(); }
    /** A real (bundled) button drawable that Java draws; framework chrome falls back to a native symbol. */
    boolean hasDrawnButton() { return buttonDrawable != null && !(buttonDrawable instanceof android.content.res.VMPlaceholderDrawable); }
    private int buttonWidth() { return hasDrawnButton() ? Math.max(0, buttonDrawable.getIntrinsicWidth()) : 0; }
    public int getCompoundPaddingLeft() { return getPaddingLeft() + buttonWidth(); }
    protected void onDraw(android.graphics.Canvas c) {
        super.onDraw(c);
        if (!hasDrawnButton()) return;
        int w = buttonDrawable.getIntrinsicWidth(), h = buttonDrawable.getIntrinsicHeight();
        if (w <= 0 || h <= 0) return;
        int vg = getGravity() & android.view.Gravity.VERTICAL_GRAVITY_MASK;
        int top = vg == android.view.Gravity.BOTTOM ? getHeight() - h : vg == android.view.Gravity.CENTER_VERTICAL ? (getHeight() - h) / 2 : 0;
        buttonDrawable.setBounds(0, top, w, top + h);
        buttonDrawable.draw(c);
    }
    protected boolean indroidNeedsCanvas() { return super.indroidNeedsCanvas() || hasDrawnButton(); }
    void buttonDefaults() {
        if (getMinimumHeight() == 0) setMinimumHeight(48);
        if (getGravity() == 0) setGravity(android.view.Gravity.START | android.view.Gravity.CENTER_VERTICAL);
    }
    int indroidExtraWidth() { if (hasDrawnButton() || android.view.VMViews.overridesOnDraw(this)) return buttonWidth(); return text.length() == 0 ? 32 : 40; }
    int indroidExtraHeight() { return hasDrawnButton() ? Math.max(0, buttonDrawable.getIntrinsicHeight()) : 32; }
    protected void indroidProps(StringBuilder sb) {
        super.indroidProps(sb);
        Json.key(sb, "checked"); sb.append(checked);
        Json.key(sb, "clickable"); sb.append("true");
        // the app (or Java) draws the check mark itself: the host must not add a native one
        if (hasDrawnButton() || android.view.VMViews.overridesOnDraw(this)) { Json.key(sb, "customButton"); sb.append("true"); Json.key(sb, "textInset"); sb.append(buttonWidth()); }
    }
}
