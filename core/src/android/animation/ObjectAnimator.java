package android.animation;
import android.util.Property;
import android.view.View;
public class ObjectAnimator extends ValueAnimator {
    private android.graphics.PathMeasure pathMeasure; private TypeConverter converter;
    private Object target; private String propertyName; private Property property;
    private float[] pendingFloats;
    public static ObjectAnimator ofPropertyValuesHolder(Object target, PropertyValuesHolder... values) {
        ObjectAnimator a = new ObjectAnimator(); a.target=target;
        if (values.length > 0) { PropertyValuesHolder p=values[0]; a.propertyName=p.getPropertyName(); a.property=p.indroidProperty(); if (p.indroidFloats()!=null) { a.pendingFloats=p.indroidFloats(); a.setFloatValues(p.indroidFloats()); } else if (p.indroidInts()!=null) a.setIntValues(p.indroidInts()); }
        return a;
    }
    public void setAutoCancel(boolean cancel) {}
    public static ObjectAnimator ofFloat(Object target, String name, float... values) { ObjectAnimator a = new ObjectAnimator(); a.target=target; a.propertyName=name; a.pendingFloats=values; a.setFloatValues(values); return a; }
    public static <T> ObjectAnimator ofFloat(T target, Property<T,Float> property, float... values) { ObjectAnimator a = new ObjectAnimator(); a.target=target; a.property=property; a.pendingFloats=values; a.setFloatValues(values); return a; }
    public static ObjectAnimator ofInt(Object target, String name, int... values) { ObjectAnimator a = new ObjectAnimator(); a.target=target; a.propertyName=name; a.setIntValues(values); return a; }
    public ObjectAnimator setDuration(long d) { super.setDuration(d); return this; }
    public void start() { if (pendingFloats != null && pendingFloats.length == 1) { Float initial = null; if (property != null) { Object v=property.get(target); if (v instanceof Number) initial=Float.valueOf(((Number)v).floatValue()); } else if (target instanceof View) { View v=(View)target; if ("alpha".equals(propertyName)) initial=Float.valueOf(v.getAlpha()); else if ("translationX".equals(propertyName)) initial=Float.valueOf(v.getTranslationX()); else if ("translationY".equals(propertyName)) initial=Float.valueOf(v.getTranslationY()); else if ("scaleX".equals(propertyName)) initial=Float.valueOf(v.getScaleX()); else if ("scaleY".equals(propertyName)) initial=Float.valueOf(v.getScaleY()); else if ("rotation".equals(propertyName)) initial=Float.valueOf(v.getRotation()); else if ("x".equals(propertyName)) initial=Float.valueOf(v.getX()); else if ("y".equals(propertyName)) initial=Float.valueOf(v.getY()); } if (initial != null) setFloatValues(initial.floatValue(),pendingFloats[0]); } super.start(); }
    public static ObjectAnimator ofObject(Object target,Property property,TypeConverter converter,android.graphics.Path path) { ObjectAnimator a=new ObjectAnimator();a.target=target;a.property=property;a.converter=converter;a.pathMeasure=new android.graphics.PathMeasure(path,false);a.setFloatValues(0,1);return a; }
    public void setTarget(Object t) { target=t; } public Object getTarget() { return target; }
    public void setPropertyName(String n) { propertyName=n; } public String getPropertyName() { return propertyName; }
    public void setProperty(Property p) { property=p; }
    protected void animateValue(Object value) { if(pathMeasure!=null && property!=null) { float[] xy=new float[2];pathMeasure.getPosTan(pathMeasure.getLength()*((Number)value).floatValue(),xy,null);android.graphics.PointF point=new android.graphics.PointF(xy[0],xy[1]);property.set(target,converter==null?point:converter.convert(point));return; } if (property != null) { property.set(target, value); return; } if (!(target instanceof View) || !(value instanceof Number)) return;
        View v=(View)target; float f=((Number)value).floatValue();
        if ("alpha".equals(propertyName)) v.setAlpha(f); else if ("translationX".equals(propertyName)) v.setTranslationX(f); else if ("translationY".equals(propertyName)) v.setTranslationY(f); else if ("scaleX".equals(propertyName)) v.setScaleX(f); else if ("scaleY".equals(propertyName)) v.setScaleY(f); else if ("rotation".equals(propertyName)) v.setRotation(f); else if ("x".equals(propertyName)) v.setX(f); else if ("y".equals(propertyName)) v.setY(f);
    }
}
