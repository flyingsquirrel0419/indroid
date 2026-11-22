package android.graphics;
public class Color {
    public static final int BLACK = 0xFF000000, DKGRAY = 0xFF444444, GRAY = 0xFF888888, LTGRAY = 0xFFCCCCCC, WHITE = 0xFFFFFFFF,
        RED = 0xFFFF0000, GREEN = 0xFF00FF00, BLUE = 0xFF0000FF, YELLOW = 0xFFFFFF00, CYAN = 0xFF00FFFF, MAGENTA = 0xFFFF00FF, TRANSPARENT = 0;
    public static int rgb(int r, int g, int b) { return 0xff000000 | (r << 16) | (g << 8) | b; }
    public static int argb(int a, int r, int g, int b) { return (a << 24) | (r << 16) | (g << 8) | b; }
    public static int alpha(int c) { return c >>> 24; }
    public static int red(int c) { return (c >> 16) & 0xff; }
    public static int green(int c) { return (c >> 8) & 0xff; }
    public static int blue(int c) { return c & 0xff; }
    public static int parseColor(String s) {
        if (s.charAt(0) == '#') {
            long c = Long.parseLong(s.substring(1), 16);
            if (s.length() == 7) c |= 0xff000000L;
            else if (s.length() != 9) throw new IllegalArgumentException("Unknown color");
            return (int) c;
        }
        String n = s.toLowerCase();
        if (n.equals("red")) return RED; if (n.equals("blue")) return BLUE; if (n.equals("green")) return GREEN;
        if (n.equals("black")) return BLACK; if (n.equals("white")) return WHITE; if (n.equals("gray") || n.equals("grey")) return GRAY;
        if (n.equals("yellow")) return YELLOW; if (n.equals("cyan")) return CYAN; if (n.equals("magenta")) return MAGENTA;
        throw new IllegalArgumentException("Unknown color");
    }
    /** indroid: "#AARRGGBB" for the host. */
    public static String toHex(int c) {
        String h = Integer.toHexString(c);
        while (h.length() < 8) h = "0" + h;
        return "#" + h;
    }
 public static void colorToHSV(int color,float[] hsv){RGBToHSV(red(color),green(color),blue(color),hsv);} public static void RGBToHSV(int r,int g,int b,float[] hsv){float rf=r/255f,gf=g/255f,bf=b/255f,max=Math.max(rf,Math.max(gf,bf)),min=Math.min(rf,Math.min(gf,bf)),d=max-min,h=0;if(d!=0){if(max==rf)h=60*((gf-bf)/d%6);else if(max==gf)h=60*((bf-rf)/d+2);else h=60*((rf-gf)/d+4);if(h<0)h+=360;}hsv[0]=h;hsv[1]=max==0?0:d/max;hsv[2]=max;} public static int HSVToColor(float[] hsv){return HSVToColor(255,hsv);} public static int HSVToColor(int alpha,float[] hsv){float h=((hsv[0]%360)+360)%360,s=Math.max(0,Math.min(1,hsv[1])),v=Math.max(0,Math.min(1,hsv[2])),c=v*s,x=c*(1-Math.abs((h/60)%2-1)),m=v-c;float r=0,g=0,b=0;if(h<60){r=c;g=x;}else if(h<120){r=x;g=c;}else if(h<180){g=c;b=x;}else if(h<240){g=x;b=c;}else if(h<300){r=x;b=c;}else{r=c;b=x;}return argb(alpha,Math.round((r+m)*255),Math.round((g+m)*255),Math.round((b+m)*255));} public static Color valueOf(int c){Color v=new Color();v.value=c;return v;} private int value; public int toArgb(){return value;} public float red(){return red(value)/255f;} public float green(){return green(value)/255f;} public float blue(){return blue(value)/255f;} public float alpha(){return alpha(value)/255f;} public static float luminance(int c){double r=red(c)/255.0,g=green(c)/255.0,b=blue(c)/255.0;r=r<=.03928?r/12.92:Math.pow((r+.055)/1.055,2.4);g=g<=.03928?g/12.92:Math.pow((g+.055)/1.055,2.4);b=b<=.03928?b/12.92:Math.pow((b+.055)/1.055,2.4);return (float)(.2126*r+.7152*g+.0722*b);} public float luminance(){return luminance(value);} 
}
