// Simply Droplets - 3D Radial Positioning Prototype
// This file implements 3D positioning with radial coordinates for the Simply Droplets plugin

// ==============================
// UTILITY FUNCTIONS
// ==============================

// Function to plot 3D positions (top-down and side view)
~plot3DPositions = {|positions, title="3D Positions"|
    var topView = positions.collect {|pos| [pos[1], pos[0]] };  // azimuth, radius
    var sideView = positions.collect {|pos| [pos[1], pos[2]] }; // azimuth, elevation
    
    topView.plot(title + " (Top View: Azimuth/Radius)", Rect(100, 100, 400, 400));
    sideView.plot(title + " (Side View: Azimuth/Elevation)", Rect(520, 100, 400, 400));
};

// Function to generate test positions
~generateTestPositions = {|count=100, distributionMethod=\uniform|
    var positions = Array.fill(count, {
        var radius, azimuth, elevation;
        
        case
        { distributionMethod == \uniform } {
            radius = 1.0.rand;
            azimuth = 2pi.rand;
            elevation = pi.rand - (pi/2);
        }
        { distributionMethod == \spherical } {
            // Generate points evenly distributed on a sphere
            var u = 1.0.rand;
            var v = 1.0.rand;
            var theta = 2pi * u;
            var phi = acos(2 * v - 1);
            
            radius = 1.0.rand;  // Random radius
            azimuth = theta;    // Uniform around the circle
            elevation = phi - (pi/2);  // Convert from [0, pi] to [-pi/2, pi/2]
        }
        { distributionMethod == \clustered } {
            // Generate clustered points around certain positions
            var cluster = 3.rand;
            var clusterCenters = [
                [0.8, 0, 0],      // Front center
                [0.7, pi/2, 0.3],  // Right side
                [0.6, -pi/2, -0.2] // Left side
            ];
            var center = clusterCenters[cluster];
            var spread = 0.3;
            
            radius = (center[0] + (spread * 0.5 * 1.0.rand)).clip(0, 1);
            azimuth = (center[1] + (spread * pi * 1.0.rand)).mod(2pi);
            elevation = (center[2] + (spread * 0.5 * 1.0.rand)).clip(-pi/2, pi/2);
        }
        { distributionMethod == \shell } {
            // Generate points on the surface of a sphere (fixed radius)
            var u = 1.0.rand;
            var v = 1.0.rand;
            var theta = 2pi * u;
            var phi = acos(2 * v - 1);
            
            radius = 0.8;          // Fixed radius
            azimuth = theta;       // Uniform around the circle
            elevation = phi - (pi/2);  // Convert from [0, pi] to [-pi/2, pi/2]
        };
        
        [radius, azimuth, elevation];
    });
    
    positions;
};

// Function to convert radial coordinates to Cartesian
~radialToCartesian = {|radial|
    var radius = radial[0];
    var azimuth = radial[1];
    var elevation = radial[2];
    
    var x = radius * cos(elevation) * cos(azimuth);
    var y = radius * cos(elevation) * sin(azimuth);
    var z = radius * sin(elevation);
    
    [x, y, z];
};

// Function to convert Cartesian coordinates to radial
~cartesianToRadial = {|cartesian|
    var x = cartesian[0];
    var y = cartesian[1];
    var z = cartesian[2];
    
    var radius = sqrt((x * x) + (y * y) + (z * z));
    var azimuth = atan2(y, x);
    var elevation = asin(z / radius);
    
    [radius, azimuth, elevation];
};

// Function to get the polar opposite position
~getPolarOpposite = {|radial|
    var radius = radial[0];
    var azimuth = radial[1];
    var elevation = radial[2];
    
    // Add π to azimuth and negate elevation for perfect polar opposite
    var oppositeAzimuth = (azimuth + pi) % (2 * pi);
    var oppositeElevation = -elevation;
    
    [radius, oppositeAzimuth, oppositeElevation];
};

// ==============================
// 3D TO STEREO RENDERING
// ==============================

// Render a 3D position to stereo (returns [left, right] gain)
~renderToStereo = {|radial, mode=\simple|
    var radius = radial[0];
    var azimuth = radial[1];
    var elevation = radial[2];
    var left, right, depth, elevationFactor;
    
    case
    // Simple panning based on azimuth, with depth based on radius
    { mode == \simple } {
        var pan = sin(azimuth) * 0.5 + 0.5;  // Convert azimuth to 0-1 pan value
        var depthAttenuation = 1.0 - (radius * 0.7);  // Distant sounds are quieter (up to 70% reduction)
        
        left = cos(pan * pi/2) * depthAttenuation;
        right = sin(pan * pi/2) * depthAttenuation;
    }
    
    // More complex model with elevation affecting the stereo image
    { mode == \advanced } {
        // Use cosine-based panning law for smoother energy distribution
        var pan = sin(azimuth) * 0.5 + 0.5;  // Convert azimuth to 0-1 pan value
        
        // Depth attenuation: distant sounds are quieter
        var depthAttenuation = 0.3 + (0.7 * (1.0 - radius));  // Scale from 0.3 to 1.0
        
        // Elevation affects the stereo width - higher sounds are wider
        var elevationFactor = 0.5 + (sin(elevation) * 0.5);  // 0-1 based on elevation
        var effectivePan = (pan - 0.5) * elevationFactor + 0.5;  // Apply elevation to pan
        
        // Apply the panning law
        left = cos(effectivePan * pi/2) * depthAttenuation;
        right = sin(effectivePan * pi/2) * depthAttenuation;
    }
    
    // HRTF-inspired approximation with more spatial cues
    { mode == \spatial } {
        var pan = sin(azimuth) * 0.5 + 0.5;  // Convert azimuth to 0-1 pan value
        var depthAttenuation = 0.3 + (0.7 * (1.0 - radius));  // Scale from 0.3 to 1.0
        
        // Base stereo position
        left = cos(pan * pi/2);
        right = sin(pan * pi/2);
        
        // Add depth cues (slight lowpass filtering for distant sounds)
        left = left * depthAttenuation;
        right = right * depthAttenuation;
        
        // Add elevation cues (simplified HRTF approximation)
        // Higher sounds have more high frequency content and slight phase modification
        elevationFactor = sin(elevation) * 0.3;  // -0.3 to 0.3 range
        
        // For higher sounds (positive elevation), boost right channel slightly
        // For lower sounds (negative elevation), boost left channel slightly
        // This creates a subtle height impression
        left = left * (1.0 - elevationFactor);
        right = right * (1.0 + elevationFactor);
    }
    
    [left, right];
};

// Function to play a sound at a 3D position
~playAtPosition = {|radial, renderMode=\simple, freq=440, dur=0.5|
    var pos = radial;
    var stereoGain = ~renderToStereo.value(pos, renderMode);
    
    {
        var sig = SinOsc.ar(freq) * EnvGen.kr(Env.perc(0.01, dur), doneAction: 2);
        Out.ar(0, sig * stereoGain);
    }.play;
};

// Function to play a sequence of positions
~playPositionSequence = {|positions, renderMode=\simple, baseFreq=440, durs=0.2|
    Task({
        positions.do {|pos, i|
            var freq = baseFreq * (1 + (i % 12) / 12);  // Simple scale
            ~playAtPosition.value(pos, renderMode, freq, durs);
            durs.wait;
        };
    }).play;
};

// Function to play a continuous sound that moves along a path
~playMovingSound = {|path, dur=5.0, renderMode=\simple, freq=440|
    {
        var numPoints = path.size;
        var sig = SinOsc.ar(freq) * EnvGen.kr(Env.sine(dur), doneAction: 2);
        var pos = Line.kr(0, numPoints - 0.001, dur);
        var posIndex = pos.floor;
        var posFrac = pos - posIndex;
        var position, currentPos, nextPos, interpPos, stereoGain;
        
        // Linear interpolation between path points
        currentPos = Select.kr(posIndex, path.flop[0]);
        nextPos = Select.kr(posIndex + 1 % numPoints, path.flop[0]);
        var currentAzimuth = Select.kr(posIndex, path.flop[1]);
        var nextAzimuth = Select.kr(posIndex + 1 % numPoints, path.flop[1]);
        var currentElevation = Select.kr(posIndex, path.flop[2]);
        var nextElevation = Select.kr(posIndex + 1 % numPoints, path.flop[2]);
        
        // Interpolate radius
        var radius = currentPos * (1 - posFrac) + (nextPos * posFrac);
        
        // Handle wrapping for azimuth interpolation
        var azimuthDiff = nextAzimuth - currentAzimuth;
        if(azimuthDiff.abs > pi) {
            if(azimuthDiff > 0) {
                nextAzimuth = nextAzimuth - 2pi;
            } {
                nextAzimuth = nextAzimuth + 2pi;
            };
        };
        var azimuth = currentAzimuth * (1 - posFrac) + (nextAzimuth * posFrac);
        azimuth = azimuth.mod(2pi);
        
        // Interpolate elevation
        var elevation = currentElevation * (1 - posFrac) + (nextElevation * posFrac);
        
        position = [radius, azimuth, elevation];
        stereoGain = ~renderToStereo.value(position, renderMode);
        
        sig * stereoGain;
    }.play;
};

// Generate path shapes
~generatePath = {|shape=\circle, points=24|
    var path;
    
    case
    // Circle path around the listener
    { shape == \circle } {
        path = Array.fill(points, {|i|
            var angle = 2pi * i / points;
            [0.8, angle, 0.0];  // Fixed radius, varying azimuth, zero elevation
        });
    }
    
    // Spiral path that also changes elevation
    { shape == \spiral } {
        path = Array.fill(points, {|i|
            var angle = 2pi * i / points;
            var progress = i / (points - 1);  // 0 to 1
            [0.3 + (0.6 * progress), angle, -0.5 + progress];  // Varying radius and elevation
        });
    }
    
    // Figure-8 pattern
    { shape == \figure8 } {
        path = Array.fill(points, {|i|
            var angle = 2pi * i / points;
            var radius = 0.7;
            var azimuth = angle;
            var elevation = sin(2 * angle) * 0.4;  // Figure-8 pattern in elevation
            [radius, azimuth, elevation];
        });
    }
    
    // Random walk
    { shape == \randomWalk } {
        var currentPos = [0.5, 0, 0];  // Start at [radius, azimuth, elevation]
        path = Array.fill(points, {|i|
            var radius = (currentPos[0] + 0.1.rand2).clip(0.1, 1.0);
            var azimuth = (currentPos[1] + 0.5.rand2).mod(2pi);
            var elevation = (currentPos[2] + 0.1.rand2).clip(-pi/2, pi/2);
            currentPos = [radius, azimuth, elevation];
        });
    }
    
    // Custom path (example)
    { shape == \custom } {
        path = [
            [0.8, 0, 0],          // Front
            [0.7, pi/4, 0.2],     // Front-right, slightly up
            [0.6, pi/2, 0.4],     // Right, higher
            [0.7, 3pi/4, 0.2],    // Back-right, slightly up
            [0.8, pi, 0],         // Back
            [0.7, 5pi/4, -0.2],   // Back-left, slightly down
            [0.6, 3pi/2, -0.4],   // Left, lower
            [0.7, 7pi/4, -0.2],   // Front-left, slightly down
        ];
    };
    
    path;
};

// ==============================
// TESTING AND VISUALIZATION
// ==============================

// Start the server for audio testing
s.boot;

// Plot different distribution methods
(
var uniformPositions = ~generateTestPositions.value(200, \uniform);
var sphericalPositions = ~generateTestPositions.value(200, \spherical);
var clusteredPositions = ~generateTestPositions.value(200, \clustered);
var shellPositions = ~generateTestPositions.value(200, \shell);

~plot3DPositions.value(uniformPositions, "Uniform Distribution");
~plot3DPositions.value(sphericalPositions, "Spherical Distribution");
~plot3DPositions.value(clusteredPositions, "Clustered Distribution");
~plot3DPositions.value(shellPositions, "Shell Distribution");
)

// Test coordinate conversions
(
var testRadial = [0.8, pi/4, pi/6];  // radius, azimuth, elevation
var cartesian = ~radialToCartesian.value(testRadial);
var backToRadial = ~cartesianToRadial.value(cartesian);

"Original Radial:".postln;
testRadial.postln;
"Converted to Cartesian:".postln;
cartesian.postln;
"Back to Radial:".postln;
backToRadial.postln;

// Test polar opposite
var opposite = ~getPolarOpposite.value(testRadial);
"Polar Opposite:".postln;
opposite.postln;
)

// Test stereo rendering of individual positions
(
var testPositions = [
    [0.5, 0, 0],         // Front center
    [0.5, pi/2, 0],      // Right
    [0.5, pi, 0],        // Back
    [0.5, 3pi/2, 0],     // Left
    [1.0, 0, 0],         // Distant front
    [0.2, 0, 0],         // Close front
    [0.5, 0, pi/3],      // Front above
    [0.5, 0, -pi/3],     // Front below
];

testPositions.do {|pos, i|
    var stereo = ~renderToStereo.value(pos, \advanced);
    [i, pos, stereo].postln;
};
)

// Test audio playback at different positions
(
var testPositions = [
    [0.5, 0, 0],         // Front center
    [0.5, pi/2, 0],      // Right
    [0.5, pi, 0],        // Back
    [0.5, 3pi/2, 0],     // Left
    [1.0, 0, 0],         // Distant front
    [0.2, 0, 0]          // Close front
];

fork {
    testPositions.do {|pos, i|
        ~playAtPosition.value(pos, \advanced, 440, 0.5);
        0.7.wait;
    };
}
)

// Test audio playback through a movement path
(
var circlePath = ~generatePath.value(\circle, 24);
~playMovingSound.value(circlePath, 5.0, \advanced, 440);
)

(
var spiralPath = ~generatePath.value(\spiral, 24);
~playMovingSound.value(spiralPath, 5.0, \advanced, 440);
)

// ==============================
// INTERACTIVE TESTING
// ==============================

// GUI for testing 3D positioning
(
var win = Window("3D Position Tester", Rect(100, 100, 800, 600)).front;
var radiusSlider, azimuthSlider, elevationSlider;
var positionView, renderModeMenu, pathMenu;
var playButton, pathButton, stopButton;
var currentPosition, renderMode;
var radiusText, azimuthText, elevationText;
var redrawPosition;
var drawArea;

// Create a view for visualizing the position
drawArea = UserView(win, Rect(300, 20, 480, 480))
    .background_(Color.black)
    .drawFunc_({|view|
        var center = view.bounds.center;
        var radius = view.bounds.width * 0.45;
        var userRadius, userAzimuth, userElevation;
        var x, y, size;
        
        if(currentPosition.notNil) {
            userRadius = currentPosition[0];
            userAzimuth = currentPosition[1];
            userElevation = currentPosition[2];
            
            // Draw reference circles
            Pen.strokeColor = Color.gray(0.3);
            Pen.addArc(center, radius * 0.25, 0, 2pi);
            Pen.stroke;
            Pen.addArc(center, radius * 0.5, 0, 2pi);
            Pen.stroke;
            Pen.addArc(center, radius * 0.75, 0, 2pi);
            Pen.stroke;
            Pen.addArc(center, radius, 0, 2pi);
            Pen.stroke;
            
            // Draw coordinate axes
            Pen.strokeColor = Color.gray(0.5);
            Pen.moveTo(center.x - radius, center.y);
            Pen.lineTo(center.x + radius, center.y);
            Pen.stroke;
            Pen.moveTo(center.x, center.y - radius);
            Pen.lineTo(center.x, center.y + radius);
            Pen.stroke;
            
            // Draw the position (top-down view)
            x = center.x + (cos(userAzimuth) * userRadius * radius);
            y = center.y + (sin(userAzimuth) * userRadius * radius);
            
            // Size varies with elevation
            size = 10 * (1 + sin(userElevation));
            
            // Color based on elevation (blue below, green at horizon, red above)
            Pen.fillColor = Color.new(
                0.5 + (sin(userElevation) * 0.5),  // Red: more for higher elevation
                0.5 - (sin(userElevation).abs * 0.3),  // Green: more at horizon
                0.5 - (sin(userElevation) * 0.5)   // Blue: more for lower elevation
            );
            
            Pen.addOval(Rect(x - (size/2), y - (size/2), size, size));
            Pen.fill;
            
            // Draw a line from center to position
            Pen.strokeColor = Color.gray(0.6);
            Pen.moveTo(center);
            Pen.lineTo(Point(x, y));
            Pen.stroke;
            
            // Add cross at center
            Pen.strokeColor = Color.white;
            Pen.moveTo(center.x - 5, center.y);
            Pen.lineTo(center.x + 5, center.y);
            Pen.stroke;
            Pen.moveTo(center.x, center.y - 5);
            Pen.lineTo(center.x, center.y + 5);
            Pen.stroke;
            
            // Draw labels for front/back/left/right
            Pen.stringAtPoint("Front", Point(center.x - 15, center.y - radius - 20));
            Pen.stringAtPoint("Back", Point(center.x - 15, center.y + radius + 5));
            Pen.stringAtPoint("Left", Point(center.x - radius - 30, center.y - 10));
            Pen.stringAtPoint("Right", Point(center.x + radius + 5, center.y - 10));
        }
    });

// GUI elements for radius, azimuth, and elevation
StaticText(win, Rect(20, 20, 150, 20)).string_("Radius (Distance):");
radiusText = StaticText(win, Rect(250, 20, 50, 20)).string_("0.5");
radiusSlider = Slider(win, Rect(20, 40, 220, 20))
    .value_(0.5)  // Initial value
    .action_({|slider|
        var val = slider.value;
        currentPosition[0] = val;
        radiusText.string = val.round(0.01);
        drawArea.refresh;
    });

StaticText(win, Rect(20, 70, 150, 20)).string_("Azimuth (Horizontal Angle):");
azimuthText = StaticText(win, Rect(250, 70, 50, 20)).string_("0.0");
azimuthSlider = Slider(win, Rect(20, 90, 220, 20))
    .value_(0)  // Initial value
    .action_({|slider|
        var val = slider.value * 2pi;
        currentPosition[1] = val;
        azimuthText.string = (val / pi).round(0.01) ++ "π";
        drawArea.refresh;
    });

StaticText(win, Rect(20, 120, 150, 20)).string_("Elevation (Vertical Angle):");
elevationText = StaticText(win, Rect(250, 120, 50, 20)).string_("0.0");
elevationSlider = Slider(win, Rect(20, 140, 220, 20))
    .value_(0.5)  // Initial value (center for zero elevation)
    .action_({|slider|
        var val = (slider.value - 0.5) * pi;  // Map 0-1 to -π/2 to π/2
        currentPosition[2] = val;
        elevationText.string = (val / pi).round(0.01) ++ "π";
        drawArea.refresh;
    });

// Render mode selection
StaticText(win, Rect(20, 180, 150, 20)).string_("Render Mode:");
renderModeMenu = PopUpMenu(win, Rect(20, 200, 150, 20))
    .items_(["simple", "advanced", "spatial"])
    .action_({|menu|
        renderMode = menu.items[menu.value].asSymbol;
    });

// Path shape selection
StaticText(win, Rect(20, 230, 150, 20)).string_("Path Shape:");
pathMenu = PopUpMenu(win, Rect(20, 250, 150, 20))
    .items_(["circle", "spiral", "figure8", "randomWalk", "custom"])
    .action_({|menu|
        // Nothing to do here, the selection is used when the path button is pressed
    });

// Buttons
playButton = Button(win, Rect(20, 290, 100, 30))
    .states_([["Play Sound"]])
    .action_({
        ~playAtPosition.value(currentPosition, renderMode, 440, 0.5);
    });

pathButton = Button(win, Rect(130, 290, 100, 30))
    .states_([["Play Path"]])
    .action_({
        var pathShape = pathMenu.items[pathMenu.value].asSymbol;
        var path = ~generatePath.value(pathShape, 24);
        ~playMovingSound.value(path, 5.0, renderMode, 440);
    });

stopButton = Button(win, Rect(240, 290, 50, 30))
    .states_([["Stop"]])
    .action_({
        s.freeAll;
    });

// Position presets
StaticText(win, Rect(20, 340, 150, 20)).string_("Position Presets:");
Button(win, Rect(20, 360, 80, 25))
    .states_([["Front"]])
    .action_({
        currentPosition = [0.5, 0, 0];
        radiusSlider.value = 0.5;
        azimuthSlider.value = 0;
        elevationSlider.value = 0.5;
        radiusText.string = "0.5";
        azimuthText.string = "0.0π";
        elevationText.string = "0.0π";
        drawArea.refresh;
    });

Button(win, Rect(110, 360, 80, 25))
    .states_([["Right"]])
    .action_({
        currentPosition = [0.5, pi/2, 0];
        radiusSlider.value = 0.5;
        azimuthSlider.value = 0.25;
        elevationSlider.value = 0.5;
        radiusText.string = "0.5";
        azimuthText.string = "0.5π";
        elevationText.string = "0.0π";
        drawArea.refresh;
    });

Button(win, Rect(200, 360, 80, 25))
    .states_([["Back"]])
    .action_({
        currentPosition = [0.5, pi, 0];
        radiusSlider.value = 0.5;
        azimuthSlider.value = 0.5;
        elevationSlider.value = 0.5;
        radiusText.string = "0.5";
        azimuthText.string = "1.0π";
        elevationText.string = "0.0π";
        drawArea.refresh;
    });

Button(win, Rect(20, 390, 80, 25))
    .states_([["Left"]])
    .action_({
        currentPosition = [0.5, 3pi/2, 0];
        radiusSlider.value = 0.5;
        azimuthSlider.value = 0.75;
        elevationSlider.value = 0.5;
        radiusText.string = "0.5";
        azimuthText.string = "1.5π";
        elevationText.string = "0.0π";
        drawArea.refresh;
    });

Button(win, Rect(110, 390, 80, 25))
    .states_([["Above"]])
    .action_({
        currentPosition = [0.5, 0, pi/3];
        radiusSlider.value = 0.5;
        azimuthSlider.value = 0;
        elevationSlider.value = 0.5 + (1/6);
        radiusText.string = "0.5";
        azimuthText.string = "0.0π";
        elevationText.string = "0.33π";
        drawArea.refresh;
    });

Button(win, Rect(200, 390, 80, 25))
    .states_([["Below"]])
    .action_({
        currentPosition = [0.5, 0, -pi/3];
        radiusSlider.value = 0.5;
        azimuthSlider.value = 0;
        elevationSlider.value = 0.5 - (1/6);
        radiusText.string = "0.5";
        azimuthText.string = "0.0π";
        elevationText.string = "-0.33π";
        drawArea.refresh;
    });

// Add explanatory text
TextView(win, Rect(300, 510, 480, 80))
    .string_(
        "3D Radial Positioning in Simply Droplets\n\n" ++
        "This prototype demonstrates the 3D positioning system using radial coordinates.\n" ++
        "• Radius: Distance from the center (0.0 - 1.0)\n" ++
        "• Azimuth: Horizontal angle around the listener (0 - 2π)\n" ++
        "• Elevation: Vertical angle from horizontal plane (-π/2 to π/2)\n\n" ++
        "The top-down view shows position in the horizontal plane, with color and size\n" ++
        "indicating elevation. Try the different render modes and path shapes!"
    )
    .editable_(false);

// Initialize with default values
currentPosition = [0.5, 0, 0];  // [radius, azimuth, elevation]
renderMode = \advanced;
renderModeMenu.value = 1;  // Set to advanced by default

// Draw the initial position
drawArea.refresh;
)

// ==============================
// NOTES AND OBSERVATIONS
// ==============================

/*
3D Positioning findings:

1. The radial coordinate system (radius, azimuth, elevation) provides an
   intuitive way to work with spatial audio, particularly for the polar
   opposite calculation which is trivial in this system.

2. Even with basic stereo rendering techniques, convincing spatial effects
   can be achieved:
   - Azimuth maps naturally to stereo panning
   - Radius affects overall gain (distant sounds are quieter)
   - Elevation can be creatively mapped to subtle stereo balance alterations

3. The 'advanced' rendering mode creates a more convincing spatial image by:
   - Using a cosine-based panning law for smoother energy distribution
   - Scaling stereo width based on elevation (higher sounds = wider stereo)
   - Applying different attenuation curves for distance

4. For clustered distributions of droplets, the resulting spatial image
   creates a sense of environment or space that feels more natural than
   uniform distributions.

5. Path-based movement adds a significant dynamic element to the spatial
   rendering, creating engaging and evolving soundscapes.

6. The most effective balance between spatial clarity and stereo compatibility
   was achieved with the 'advanced' rendering mode; the 'spatial' mode creates
   interesting effects but requires careful tuning to avoid phase issues.

7. Converting back and forth between radial and Cartesian coordinates works 
   seamlessly, which will be important for path-based movement calculations.

Key implementation insights:
- Radial coordinates simplify many spatial audio calculations
- Elevation is the most challenging dimension to render convincingly in stereo
- Movement along paths creates more engaging spatial effects than static positioning
- Simple stereo rendering techniques can create surprisingly effective 3D impression
*/
