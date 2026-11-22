package android.animation;
import android.util.Property;
public class PropertyValuesHolder {
    private String name; private Property property; private float[] floats; private int[] ints;
    private PropertyValuesHolder(String name) { this.name=name; }
    public static PropertyValuesHolder ofFloat(String name,float... values) { PropertyValuesHolder p=new PropertyValuesHolder(name);p.floats=values;return p; }
    public static PropertyValuesHolder ofInt(String name,int... values) { PropertyValuesHolder p=new PropertyValuesHolder(name);p.ints=values;return p; }
    public static PropertyValuesHolder ofFloat(Property property,float... values) { PropertyValuesHolder p=ofFloat(property.getName(),values);p.property=property;return p; }
    public static PropertyValuesHolder ofInt(Property property,int... values) { PropertyValuesHolder p=ofInt(property.getName(),values);p.property=property;return p; }
    public String getPropertyName(){return name;} public void setPropertyName(String name){this.name=name;} public void setFloatValues(float... values){floats=values;ints=null;} public void setIntValues(int... values){ints=values;floats=null;}
    public PropertyValuesHolder clone() { PropertyValuesHolder p=new PropertyValuesHolder(name); p.property=property; p.floats=floats==null?null:floats.clone(); p.ints=ints==null?null:ints.clone(); return p; }
    float[] indroidFloats() { return floats; } int[] indroidInts() { return ints; } Property indroidProperty() { return property; }
}
