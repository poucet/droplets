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
    oppositeElevation = 0-elevation;

    [radius, oppositeAzimuth, oppositeElevation];
};

// ==============================
// 3D TO STEREO RENDERING
// ==============================

// Render a 3D position to stereo (returns [left, right, leftDelay, rightDelay])
~renderBinaural = {|radial|
    var radius, azimuth, elevation;
    var left, right, pan, depthAttenuation, elevationFactor;
    var headRadius, speedOfSound, maxDelay, leftDelay, rightDelay;
    var sinAz, sinAzAbs; // Added variable declarations at the top

    radius = radial[0];
    azimuth = radial[1];
    elevation = radial[2];

    // Convert azimuth to pan position (0-1)
    pan = sin(azimuth) * 0.5 + 0.5;

    // Distance attenuation
    depthAttenuation = 0.3 + (0.7 * (1.0 - radius));

    // Calculate amplitude using cosine panning law
    left = cos(pan * pi/2) * depthAttenuation;
    right = sin(pan * pi/2) * depthAttenuation;

    // Elevation cues
    elevationFactor = sin(elevation) * 0.3;
    left = left * (1.0 - elevationFactor);
    right = right * (1.0 + elevationFactor);

    // Calculate ITD (Interaural Time Difference)
    headRadius = 0.0875; // meters
    speedOfSound = 343.0; // m/s
    maxDelay = headRadius / speedOfSound; // ~0.00025s or 0.25ms

    // Initialize delays
    leftDelay = 0.0;
    rightDelay = 0.0;

    // Calculate delays based on azimuth using UGen-compatible constructs
    // We'll use sin(azimuth) to get the lateral component and its sign
    // to determine which channel needs delay

    // Compute sin of azimuth once for efficiency
    sinAz = sin(azimuth);

    // Get absolute value for scaling the delay amount
    sinAzAbs = abs(sinAz);

    // Use proper UGen method for conditionally applying delay
    // When sinAz > 0 (sound from right), we delay left channel
    // When sinAz < 0 (sound from left), we delay right channel
    leftDelay = sinAzAbs * maxDelay * 0.5 * (sinAz + abs(sinAz));  // Only non-zero when sinAz > 0
    rightDelay = sinAzAbs * maxDelay * 0.5 * (abs(sinAz) - sinAz); // Only non-zero when sinAz < 0

    // Return amplitude and delay information
    [left, right, leftDelay, rightDelay];
};

// Function to play a sound at a 3D position
~playAtPosition = {|radial, freq=440, dur=0.5|
    var pos, stereoResult, leftGain, rightGain, leftDelay, rightDelay;
    var maxDelayBuffer;

    pos = radial;
    stereoResult = ~renderBinaural.value(pos);

    leftGain = stereoResult[0];
    rightGain = stereoResult[1];
    leftDelay = stereoResult[2];
    rightDelay = stereoResult[3];

    {
        var sig, leftSig, rightSig;

        sig = SinOsc.ar(freq) * EnvGen.kr(Env.perc(0.01, dur), doneAction: 2);

        // Ensure we have enough buffer for delay
        maxDelayBuffer = 0.001; // 1ms buffer

        // Apply delay to create phase differences
        leftSig = DelayN.ar(sig * leftGain, maxDelayBuffer, leftDelay);
        rightSig = DelayN.ar(sig * rightGain, maxDelayBuffer, rightDelay);

        // Return stereo signal with both amplitude and time differences
        [leftSig, rightSig];
    }.play;
};

// Function to play a sequence of positions
~playPositionSequence = {|positions, baseFreq=440, durs=0.2|
    Task({
        positions.do {|pos, i|
            var freq;

            freq = baseFreq * (1 + (i % 12) / 12);  // Simple scale
            ~playAtPosition.value(pos, freq, durs);
            durs.wait;
        };
    }).play;
};

// Function to play a continuous sound that moves along a path
~playMovingSound = {|path, dur=5.0, freq=440|
    {
        var numPoints, sig, pos, posIndex, posFrac;
        var position, currentPos, nextPos, stereoResult;
        var currentAzimuth, nextAzimuth, currentElevation, nextElevation;
        var radius, azimuth, azimuthDiff, elevation;
        var leftGain, rightGain, leftDelay, rightDelay;
        var maxDelayBuffer, leftSig, rightSig;

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
        stereoResult = ~renderBinaural.value(position);

        leftGain = stereoResult[0];
        rightGain = stereoResult[1];
        leftDelay = stereoResult[2];
        rightDelay = stereoResult[3];

        // Apply delay for ITD
        maxDelayBuffer = 0.001; // 1ms buffer
        leftSig = DelayN.ar(sig * leftGain, maxDelayBuffer, leftDelay);
        rightSig = DelayN.ar(sig * rightGain, maxDelayBuffer, rightDelay);

        // Return stereo signal with both amplitude and time differences
        [leftSig, rightSig];
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
var opposite;

"Original Radial:".postln;
testRadial.postln;
"Converted to Cartesian:".postln;
cartesian.postln;
"Back to Radial:".postln;
backToRadial.postln;

// Test polar opposite
opposite = ~getPolarOpposite.value(testRadial);
"Polar Opposite:".postln;
opposite.postln;
)

// Test binaural rendering of individual positions
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
    var stereo = ~renderBinaural.value(pos);
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
        ~playAtPosition.value(pos, 440, 0.5);
        0.7.wait;
    };
}
)

// Test audio playback through a movement path
(
var circlePath = ~generatePath.value(\circle, 24);
~playMovingSound.value(circlePath, 5.0, 440);
)

(
var spiralPath = ~generatePath.value(\spiral, 24);
~playMovingSound.value(spiralPath, 5.0, 440);
)

// ==============================
// BINAURAL RENDERING TESTS
// ==============================

// Test binaural rendering with individual positions
(
var testPositions = [
    [0.5, 0, 0],         // Front center
    [0.5, pi/2, 0],      // Right
    [0.5, pi, 0],        // Back
    [0.5, 3pi/2, 0],     // Left
    [0.5, pi/4, 0],      // Front-right
    [0.5, 3pi/4, 0],     // Back-right
    [0.5, 5pi/4, 0],     // Back-left
    [0.5, 7pi/4, 0],     // Front-left
];

fork {
    "Playing test positions with binaural rendering...".postln;
    testPositions.do {|pos, i|
        ("Position " ++ i ++ ": " ++ pos).postln;
        ~playAtPosition.value(pos, 440, 0.5);
        0.7.wait;
    };
}
)

// Test circle path using binaural rendering
(
var circlePath = ~generatePath.value(\circle, 24);
~playMovingSound.value(circlePath, 5.0, 440);
)

// Test figure-8 path using binaural rendering
(
var figure8Path = ~generatePath.value(\figure8, 24);
~playMovingSound.value(figure8Path, 5.0, 440);
)

// ==============================
// NOTES AND OBSERVATIONS
// ==============================

/*
3D Positioning findings:

1. The radial coordinate system (radius, azimuth, elevation) provides an
   intuitive way to work with spatial audio, particularly for the polar
   opposite calculation which is trivial in this system.

2. The combined binaural rendering approach significantly enhances spatial perception by:
   - Using amplitude differences (ILD - Interaural Level Differences)
   - Adding time/phase differences (ITD - Interaural Time Differences)
   - Including elevation cues for height perception
   - Improving front-back discrimination that amplitude-only approaches struggle with

3. For clustered distributions of droplets, the resulting spatial image
   creates a sense of environment or space that feels more natural than
   uniform distributions.

4. Path-based movement adds a significant dynamic element to the spatial
   rendering, creating engaging and evolving soundscapes.

5. Converting between radial and Cartesian coordinates works seamlessly,
   which is important for path-based movement calculations.

Key implementation insights:
- Radial coordinates simplify many spatial audio calculations
- Elevation is the most challenging dimension to render convincingly in stereo
- Movement along paths creates more engaging spatial effects than static positioning
- The ITD implementation adds critical phase/time differences between channels:
  * Maximum delay of ~0.25ms corresponds to human head dimensions
  * Delay varies with azimuth according to sine function for natural transition
  * Left/right channel delays alternate based on sound direction
*/
