import android.content.res.VMXml;
import android.graphics.Canvas;
import android.graphics.PathParser;
import android.graphics.drawable.VectorDrawable;
import android.graphics.drawable.Drawable;

public class VectorSmoke {
 private static void check(boolean ok,String message) { if(!ok)throw new RuntimeException(message); }
 private static String attr(String name,int id,String value){return "\u0002"+name+"\u0005"+id+"\u0004"+value;}
 public static void main(String[] args) {
  String xml="\u0001vector"+attr("width",0x01010159,"d:24dp")+attr("height",0x01010155,"d:12dp")+attr("viewportWidth",0x01010402,"f:12")+attr("viewportHeight",0x01010403,"f:12")+attr("alpha",0x0101031f,"f:0.5")
   +"\u0001group"+attr("translateX",0x0101045a,"f:2")+attr("rotation",0x01010326,"f:90")
   +"\u0001path"+attr("pathData",0x01010405,"s:M1 1 l2 0 h1 v2 q1 1 2 0 t2 0 a1 1 0 011 1 z")+attr("fillColor",0x01010404,"c:-65536")+attr("strokeColor",0x01010406,"c:-16776961")+attr("strokeWidth",0x01010407,"f:1")+attr("fillType",0x0101051e,"i:1")+"\u0003\u0003\u0003";
  VectorDrawable v=VectorDrawable.indroidInflate(VMXml.parse(xml),null,null);check(v.getIntrinsicWidth()==24&&v.getIntrinsicHeight()==12,"intrinsic size");
  v.setBounds(2,3,26,15);Canvas c=new Canvas();v.draw(c);String ops=c.indroidOps();System.out.println(ops);
  check(ops.indexOf("M 1 1 L 3 1 L 4 1 L 4 3 Q 5 4 6 3 Q 7 2 8 3 C")>=0,"relative commands or arc");
  check(ops.indexOf(" 9 4 Z")>=0,"arc endpoint");check(ops.indexOf("#80ff0000")>=0,"alpha");check(ops.indexOf(",1]")>=0,"fill type");
  Drawable copy=v.getConstantState().newDrawable();copy.setTint(0xff00ff00);copy.setBounds(0,0,24,12);Canvas other=new Canvas();copy.draw(other);check(other.indroidOps().indexOf("#8000ff00")>=0,"copy tint");Canvas original=new Canvas();v.draw(original);check(original.indroidOps().indexOf("#80ff0000")>=0,"copy independence");
  String p=PathParser.createPathFromPathData("M1.5-2.3e-1.4 2 C1 2 3 4 5 6 s2 3 4 5 Q1 2 3 4 t5 6 Z m1 2 A0 2 0 0 1 4 5").indroidPath();
  check(p.indexOf("M 1.5 -0.23 L 0.4 2 C 1 2 3 4 5 6 C 7 8 7 9 9 11 Q 1 2 3 4 Q 5 6 8 10 Z M 2.5 1.77 L 4 5")>=0,"number parsing, smooth curves or close: "+p);
  String tiny=PathParser.createPathFromPathData("M1e-8 1e8").indroidPath();check(tiny.indexOf("E")<0 && tiny.indexOf("e")<0,"path exponent: "+tiny);
  System.out.println("VectorSmoke OK");
 }
}
