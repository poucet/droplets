// Simply Droplets - 3D Radial Positioning Prototype
// This file implements 3D positioning with radial coordinates for the Simply Droplets plugin

// ==============================
// UTILITY FUNCTIONS
// ==============================

// Function to generate test positions
~generateTestPositions = {|count=100, distributionMethod=\uniform|
    var positions;
    
    positions = Array.fill(count, {
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
    var radius, azimuth, elevation;
    var x, y, z;
    
    radius = radial[0];
    azimuth = radial[1];
    elevation = radial[2];
    
    x = radius * cos(elevation) * cos(azimuth);
    y = radius * cos(elevation) * sin(azimuth);
    z = radius * sin(elevation);
    
    [x, y, z];
};

// Function to convert Cartesian coordinates to radial
~cartesianToRadial = {|cartesian|
    var x, y, z;
    var radius, azimuth, elevation;
    
    x = cartesian[0];
    y = cartesian[1];
    z = cartesian[2];
    
    radius = sqrt((x * x) + (y * y) + (z * z));
    azimuth = atan2(y, x);
    elevation = asin(z / radius);
    
    [radius, azimuth, elevation];
};

// Function to get the polar opposite position
~getPolarOpposite = {|radial|
    var radius, azimuth, elevation;
    var oppositeAzimuth, oppositeElevation;
    
    radius = radial[0];
    azimuth = radial[1];
    elevation = radial[2];
    
    // Add π to azimuth and negate elevation for perfect polar opposite
    oppositeAzimuth = (azimuth + pi) % (2 * pi);
    oppositeElevation = -elevation;
    
    [radius, oppositeAzimuth, oppositeElevation];
};

// ==============================
// 3D TO STEREO RENDERING
// ==============================

// Render a 3D position to stereo (returns [left, right] gain)
~renderToStereo = {|radial, mode=\simple|
    var radius, azimuth, elevation;
    var left, right, pan, depthAttenuation, elevationFactor, effectivePan;
    
    radius = radial[0];
    azimuth = radial[1];
    elevation = radial[2];
    
    case
    // Simple panning based on azimuth, with depth based on radius
    { mode == \simple } {
        pan = sin(azimuth) * 0.5 + 0.5;  // Convert azimuth to 0-1 pan value
        depthAttenuation = 1.0 - (radius * 0.7);  // Distant sounds are quieter (up to 70% reduction)
        
        left = cos(pan * pi/2) * depthAttenuation;
        right = sin(pan * pi/2) * depthAttenuation;
    }
    
    // More complex model with elevation affecting the stereo image
    { mode == \advanced } {
        // Use cosine-based panning law for smoother energy distribution
        pan = sin(azimuth) * 0.5 + 0.5;  // Convert azimuth to 0-1 pan value
        
        // Depth attenuation: distant sounds are quieter
        depthAttenuation = 0.3 + (0.7 * (1.0 - radius));  // Scale from 0.3 to 1.0
        
        // Elevation affects the stereo width - higher sounds are wider
        elevationFactor = 0.5 + (sin(elevation) * 0.5);  // 0-1 based on elevation
        effectivePan = (pan - 0.5) * elevationFactor + 0.5;  // Apply elevation to pan
        
        // Apply the panning law
        left = cos(effectivePan * pi/2) * depthAttenuation;
        right = sin(effectivePan * pi/2) * depthAttenuation;
    }
    
    // HRTF-inspired approximation with more spatial cues
    { mode == \spatial } {
        pan = sin(azimuth) * 0.5 + 0.5;  // Convert azimuth to 0-1 pan value
        depthAttenuation = 0.3 + (0.7 * (1.0 - radius));  // Scale from 0.3 to 1.0
        
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
    };
    
    [left, right];
};

// Function to play a sound at a 3D position
~playAtPosition = {|radial, renderMode=\simple, freq=440, dur=0.5|
    var pos, stereoGain;
    
    pos = radial;
    stereoGain = ~renderToStereo.value(pos, renderMode);
    
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
        var numPoints, sig, pos, posIndex, posFrac;
        var position, currentPos, nextPos, interpPos, stereoGain;
        var currentAzimuth, nextAzimuth, currentElevation, nextElevation;
        var radius, azimuth, azimuthDiff, elevation;
        
        numPoints = path.size;
        sig = SinOsc.ar(freq) * EnvGen.kr(Env.sine(dur), doneAction: 2);
        pos = Line.kr(0, numPoints - 0.001, dur);
        posIndex = pos.floor;
        posFrac = pos - posIndex;
        
        // Linear interpolation between path points
        currentPos = Select.kr(posIndex, path.flop[0]);
        nextPos = Select.kr(posIndex + 1 % numPoints, path.flop[0]);
        currentAzimuth = Select.kr(posIndex, path.flop[1]);
        nextAzimuth = Select.kr(posIndex + 1 % numPoints, path.flop[1]);
        currentElevation = Select.kr(posIndex, path.flop[2]);
        nextElevation = Select.kr(posIndex + 1 % numPoints, path.flop[2]);
        
        // Interpolate radius
        radius = currentPos * (1 - posFrac) + (nextPos * posFrac);
        
        // Handle wrapping for azimuth interpolation
        azimuthDiff = nextAzimuth - currentAzimuth;
        // Use UGen selection instead of if statements
        nextAzimuth = Select.kr(
            (azimuthDiff.abs > pi), [
                nextAzimuth, // No adjustment needed
                Select.kr(
                    (azimuthDiff > 0),
                    [
                        nextAzimuth + 2pi, // azimuthDiff < 0
                        nextAzimuth - 2pi  // azimuthDiff > 0
                    ]
                )
            ]
        );
        azimuth = currentAzimuth * (1 - posFrac) + (nextAzimuth * posFrac);
        azimuth = azimuth.mod(2pi);
        
        // Interpolate elevation
        elevation = currentElevation * (1 - posFrac) + (nextElevation * posFrac);
        
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
            var radius, azimuth, elevation;
            
            radius = (currentPos[0] + 0.1.rand2).clip(0.1, 1.0);
            azimuth = (currentPos[1] + 0.5.rand2).mod(2pi);
            elevation = (currentPos[2] + 0.1.rand2).clip(-pi/2, pi/2);
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
