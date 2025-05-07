
#include <FL/Fl_Double_Window.H>
#include <FL/fl_draw.H>
#include <FL/Fl_Menu_Bar.H>

#include <vector>
#include <cmath>
#include <iostream>
#include <algorithm>

using namespace std;

const int NODE_RADIUS = 3;
const int CIRC_MIN_LENGTH = 75;


class Node
{
    public:

    int x, y;

    Node(int x_, int y_)
    {
        x = x_;
        y = y_;
    }
};

class Circuit
{
    public:

    int n1, n2; 
    vector<float> x_coords;
    vector<float> y_coords;

    Circuit(int n1_, int n2_)
    {
        n1 = n1_;
        n2 = n2_;

    }

    void rotate(float x0, float y0, float angle)
    {
        for(int i=0; i<8; i++)
        {
            x_coords[i] -= x0;
            y_coords[i] -= y0;

            float xt = x_coords[i] * cos(angle) - y_coords[i] * sin(angle);
            float yt = y_coords[i] * cos(angle) + x_coords[i] * sin(angle);

            x_coords[i] = xt + x0;
            y_coords[i] = yt + y0;
        }
    }

    void update_coords(int xl, int yl, int xr, int yr)
    {
        int length = hypot(xl-xr, yl-yr);

        if(length > CIRC_MIN_LENGTH)
        {
            x_coords = {0 ,0 ,0 ,0 ,0, 0, 0, 0};
            y_coords = {0 ,0 ,0 ,0 ,0, 0, 0, 0};

            float d = 7;
            float s = 5;
            float l = hypot(xl-xr, yl-yr) * 0.5 - 3.0 * d;

            x_coords[0] = xl + l + 0.0 * d;
            x_coords[1] = xl + l + 0.5 * d;
            x_coords[2] = xl + l + 1.5 * d;
            x_coords[3] = xl + l + 2.5 * d;
            x_coords[4] = xl + l + 3.5 * d;
            x_coords[5] = xl + l + 4.5 * d;
            x_coords[6] = xl + l + 5.5 * d;
            x_coords[7] = xl + l + 6.0 * d;

            y_coords[0] = yl;
            y_coords[1] = yl + s;
            y_coords[2] = yl - s;
            y_coords[3] = yl + s;
            y_coords[4] = yl - s;
            y_coords[5] = yl + s;
            y_coords[6] = yl - s;
            y_coords[7] = yl;

            // Rotate circuit
            float angle = atan2(yr-yl,xr-xl);
            rotate(xl, yl, angle);
        }
        else
        { 
            x_coords = {};
            y_coords = {};
        }
    }
};

bool is_unique(vector<Circuit> circuits, int k)
{
    // Count occurence of 'k'
    int num_k = 0;
    for(Circuit c : circuits)
    {
        if(k == c.n1 || k == c.n2)
        {
            num_k++;
        }
    }

    if(num_k > 1)
    {
        return false;
    }
    else
    { 
        return true;
    }
}

class Drawing : public Fl_Widget
{
    private:
    
    int xc, yc, hc, wc;

    int moving_node;

    public:

    vector<Node> nodes;
    vector<Circuit> circuits;

    Drawing(int X,int Y,int W,int H) : Fl_Widget(X,Y,W,H)
    {
        reset_clip_region();
    }

    void reset_clip_region()
    {
        xc = x();
        yc = y();
        hc = h();
        wc = w();
    }

    void update_clip_region()
    {
        // Get coordinates of last two nodes added
        int x0 = nodes.back().x;
        int y0 = nodes.back().y;
        int x1 = nodes.end()[-2].x;
        int y1 = nodes.end()[-2].y;

        // Set height and widht of clipping region
        hc = abs(y0-y1) + 6 * NODE_RADIUS;
        wc = abs(x0-x1) + 6 * NODE_RADIUS;

        // Set coordinates for upper left corner
        // of clipping region
        if(x0 < x1)
        {
            xc = x0 - 3 * NODE_RADIUS;
        }
        else
        {
            xc = x1 - 3 * NODE_RADIUS;  
        }

        if(y0 < y1)
        {
            yc = y0 - 3 * NODE_RADIUS;
        }
        else
        {
            yc = y1 - 3 * NODE_RADIUS;
        }
    }

    void remove_overlap(int k)
    {
        for(int i=0; i<nodes.size()-1; i++)
        {
            if(nodes[i].x == nodes[k].x && nodes[i].y == nodes[k].y)
            {
                //nodes.pop_back(); // change to remove index 'k'
                nodes.erase(nodes.begin() + k);
                for(Circuit& c : circuits)
                {
                    if(c.n1 == k)
                    {
                        c.n1 = i;
                    }
                    if(c.n2 == k)
                    {
                        c.n2 = i;
                    }
                }
                break;
            }
        }
    }

    void remove_unvalid()
    {
        // Find circuits that are too short!
        vector<int> too_short;
        for(int i=0; i<circuits.size(); i++)
        {
            if(hypot(nodes[circuits[i].n1].x - nodes[circuits[i].n2].x, nodes[circuits[i].n1].y - nodes[circuits[i].n2].y) < CIRC_MIN_LENGTH)
            {
                too_short.push_back(i);
            }
        }

        // Sort in descending order
        sort(too_short.begin(), too_short.end(), greater<>());

        // Remove circuits that are too short!
        for(int i=0; i<too_short.size(); i++)
        {
            // Test left-side of circuit node 'n1'
            if(is_unique(circuits, circuits[too_short[i]].n1))
            {
                // If there is no other circuit connected to the node remove it
                nodes.erase(nodes.begin() + circuits[too_short[i]].n1);
            }
            // Test right-side of circuit node 'n2'
            if(is_unique(circuits, circuits[too_short[i]].n2))
            {
                // If there is no other circuit connected to the node remove it
                nodes.erase(nodes.begin() + circuits[too_short[i]].n2);
            }
            circuits.erase(circuits.begin() + too_short[i]);
        }
    }

    int on_node(int x_, int y_)
    {
      int return_value = -1;
      for(int i=0; i<nodes.size(); i++)
      {
        if(hypot(nodes[i].x - x_, nodes[i].y - y_) < 2 * NODE_RADIUS)
        {
          return_value = i;
          break;
        }
      }
      return return_value;
    }

    void node_snap(int j, int x_, int y_)
    {
        for(int i=0; i<nodes.size()-2; i++)
        {
            if(hypot(nodes[i].x - x_, nodes[i].y - y_) < 10)
            {
                nodes[j].x = nodes[i].x;
                nodes[j].y = nodes[i].y;
                break;
            }
        }
    }

    void move_node(int i, int x_, int y_)
    {
        nodes[i].x = x_;
        nodes[i].y = y_;
                
        node_snap(i, x_, y_);
    }

    void draw()
    {
        //fl_push_clip(x(),y(),w(),h());
        fl_push_clip(xc,yc,wc,hc);
        //fl_push_clip(x0-NODE_RADIUS,y0-NODE_RADIUS,abs(x0-x1)+10,abs(y0-y1)+10);

        fl_color(FL_WHITE);
        //fl_rectf(x(),y(),w(),h());
        fl_rectf(xc, yc, wc, hc);

        fl_color(FL_BLACK);

        // Draw nodes!
        for(Node& n : nodes)
        {
            fl_begin_polygon();
            fl_circle(n.x, n.y, NODE_RADIUS);
            fl_end_polygon();
        }

        for(Circuit& c : circuits)
        {
            c.update_coords(nodes[c.n1].x, nodes[c.n1].y, nodes[c.n2].x, nodes[c.n2].y);
            fl_begin_line();
            fl_vertex(nodes[c.n1].x, nodes[c.n1].y);
            for(int i=0; i<c.x_coords.size(); i++)
            {
                fl_vertex(c.x_coords[i], c.y_coords[i]);
            }
            fl_vertex(nodes[c.n2].x, nodes[c.n2].y);
            fl_end_line();
        }
    
        // Uncomment to draw clipping region
        fl_color(FL_RED);
        fl_rect(xc,yc,wc,hc);

        fl_pop_clip();
    }

    int handle(int event)
    {
        int x_mouse = Fl::event_x();
        int y_mouse = Fl::event_y();

        switch(event)
        {
            case FL_PUSH: //FL_LEFT_MOUSE:
                moving_node = on_node(x_mouse, y_mouse);
                if(moving_node < 0)
                {
                  nodes.push_back(Node(x_mouse, y_mouse));
                  nodes.push_back(Node(x_mouse, y_mouse));
                  circuits.push_back(Circuit(nodes.size()-2, nodes.size()-1));
                  moving_node = nodes.size()-1;
                }
                redraw();
                return 1;
            case FL_DRAG:
                move_node(moving_node, x_mouse, y_mouse);
                update_clip_region();
                redraw();
                return 1;
            case FL_RELEASE:
                remove_overlap(nodes.size()-1);
                remove_unvalid();
                reset_clip_region();
                redraw();
                return 1;
            default:
                return Fl_Widget::handle(event);
        }
    }
};

int main(int argc, char** argv) {

    Fl_Double_Window window(500,500);
        Fl_Menu_Bar menu(0,0,500+1,25);
            menu.add("File");
            menu.add("Options");
        Drawing drawing(0,25,500,500);
    window.end();
  
    //window.resizable(drawing);
    window.show(argc,argv);

    return Fl::run();
}
