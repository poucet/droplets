// Simply Droplets - Distributions Prototype
// This file implements and tests various distribution models for the Simply Droplets plugin

// ==============================
// UTILITY FUNCTIONS
// ==============================

// Function to plot a distribution
~plotDistribution = {|distribution, numSamples=1000, title="Distribution"|
    var samples = Array.fill(numSamples, { distribution.value });
    var plot = samples.plot(title);
    plot.plotMode = \histogram;
    plot.refresh;
};

// Function to play a distribution as pitch values
~playDistribution = {|distribution, baseFreq=440, spread=200, dur=0.1|
    {
        var trig = Impulse.kr(1/dur);
        var freq = baseFreq + (distribution.value * spread);
        var sig = SinOsc.ar(freq) * EnvGen.kr(Env.perc(0.01, dur), trig);
        Out.ar(0, sig ! 2);
    }.play;
};

// Function to create a value mapper
~createMapper = {|inMin, inMax, outMin, outMax, curve=\linear|
    {|value|
        value.linlin(inMin, inMax, outMin, outMax, \minmax);
    }
};

// ==============================
// DISTRIBUTION IMPLEMENTATIONS
// ==============================

// Basic distribution types
~distributions = (
    // Uniform distribution between 0 and 1
    uniform: { 
        1.0.rand 
    },
    
    // Gaussian (normal) distribution
    // Mean = 0.5, standard deviation adjustable
    gaussian: {|stdDev=0.15|
        var u1 = 1.0.rand;
        var u2 = 1.0.rand;
        var r = (-2 * u1.log).sqrt;
        var theta = 2pi * u2;
        var val = r * theta.sin;
        // Map from typical [-3,3] range to [0,1] and clip
        (val * stdDev + 0.5).clip(0, 1);
    },
    
    // Triangular distribution - higher probability in the middle
    triangular: {
        var a = 1.0.rand;
        var b = 1.0.rand;
        (a + b) * 0.5;
    },
    
    // Exponential distribution - more values near 0
    exponential: {|lambda=5|
        (1.0 - (-1 * lambda * 1.0.rand).exp).clip(0, 1);
    },
    
    // Bimodal distribution - two peaks
    bimodal: {|peak1=0.25, peak2=0.75, mix=0.5, stdDev=0.1|
        var dist = if(1.0.rand < mix) {
            peak1
        } {
            peak2
        };
        
        // Add gaussian noise
        (dist + (0.gaussian * stdDev)).clip(0, 1);
    },
    
    // Beta distribution - flexible shape based on alpha/beta
    beta: {|alpha=2, beta=2|
        var u, v, x;
        u = 1.0.rand;
        v = 1.0.rand;
        x = u.pow(1/alpha);
        y = v.pow(1/beta);
        if ((x + y) <= 1) { x / (x + y) } { ~distributions.beta.value(alpha, beta) };
    }
);

// ==============================
// PARAMETRIC DISTRIBUTION
// ==============================

// Create a parametric distribution that can blend between different distribution types
~parametricDistribution = {|type=\uniform, param1=0.15, param2=0.5|
    case
    { type == \uniform } { ~distributions.uniform }
    { type == \gaussian } { { ~distributions.gaussian.value(param1) } }
    { type == \triangular } { ~distributions.triangular }
    { type == \exponential } { { ~distributions.exponential.value(param1 * 10) } }
    { type == \bimodal } { { ~distributions.bimodal.value(param1, param2, 0.5, 0.1) } }
    { type == \beta } { { ~distributions.beta.value(param1 * 10, param2 * 10) } }
    { ~distributions.uniform };  // Default to uniform if unknown type
};

// ==============================
// RADIAL COORDINATE DISTRIBUTIONS
// ==============================

// Distribution specifically for radial coordinates
~radialDistribution = (
    // Radius distribution (distance from center)
    radius: {|minRadius=0, maxRadius=1, distribution|
        var value = distribution.value;
        minRadius + (value * (maxRadius - minRadius));
    },
    
    // Azimuth distribution (horizontal angle 0-2π)
    azimuth: {|minAzimuth=0, maxAzimuth=2pi, distribution|
        var value = distribution.value;
        minAzimuth + (value * (maxAzimuth - minAzimuth));
    },
    
    // Elevation distribution (vertical angle -π/2 to π/2)
    elevation: {|minElevation= -0.5pi, maxElevation=0.5pi, distribution|
        var value = distribution.value;
        minElevation + (value * (maxElevation - minElevation));
    }
);

// ==============================
// TESTING AND VISUALIZATION
// ==============================

// Start the server for audio testing
s.boot;

// Plot various distributions
(
// Simple distributions
~plotDistribution.(~distributions.uniform, 10000, "Uniform Distribution");
~plotDistribution.(~distributions.gaussian, 10000, "Gaussian Distribution");
~plotDistribution.(~distributions.triangular, 10000, "Triangular Distribution");
~plotDistribution.(~distributions.exponential, 10000, "Exponential Distribution");
~plotDistribution.(~distributions.bimodal, 10000, "Bimodal Distribution");
~plotDistribution.(~distributions.beta, 10000, "Beta Distribution");

// Parametric distribution with different types
~plotDistribution.(~parametricDistribution.(\uniform), 10000, "Parametric: Uniform");
~plotDistribution.(~parametricDistribution.(\gaussian, 0.05), 10000, "Parametric: Narrow Gaussian");
~plotDistribution.(~parametricDistribution.(\gaussian, 0.25), 10000, "Parametric: Wide Gaussian");
~plotDistribution.(~parametricDistribution.(\beta, 0.2, 0.8), 10000, "Parametric: Beta (2, 8)");
)

// Audio testing of distributions
(
var dist = ~parametricDistribution.(\gaussian, 0.15);
~playDistribution.(dist, 440, 200, 0.1);
)

// ==============================
// INTERACTIVE TESTING
// ==============================

// GUI for testing distributions
(
var win = Window("Distribution Tester", Rect(100, 100, 500, 400)).front;
var distTypeMenu, param1Slider, param2Slider, plotButton, audioButton, stopButton;
var currentDist, plotView, params;

// GUI elements
distTypeMenu = PopUpMenu(win, Rect(10, 10, 150, 20))
    .items_(["uniform", "gaussian", "triangular", "exponential", "bimodal", "beta"])
    .action_({ |menu|
        params = (
            type: menu.items[menu.value].asSymbol,
            param1: param1Slider.value,
            param2: param2Slider.value
        );
        currentDist = ~parametricDistribution.(params.type, params.param1, params.param2);
    });

StaticText(win, Rect(10, 40, 150, 20)).string_("Parameter 1:");
param1Slider = Slider(win, Rect(10, 60, 150, 20))
    .value_(0.15)
    .action_({ |slider|
        params = (
            type: distTypeMenu.items[distTypeMenu.value].asSymbol,
            param1: slider.value,
            param2: param2Slider.value
        );
        currentDist = ~parametricDistribution.(params.type, params.param1, params.param2);
    });

StaticText(win, Rect(10, 90, 150, 20)).string_("Parameter 2:");
param2Slider = Slider(win, Rect(10, 110, 150, 20))
    .value_(0.5)
    .action_({ |slider|
        params = (
            type: distTypeMenu.items[distTypeMenu.value].asSymbol,
            param1: param1Slider.value,
            param2: slider.value
        );
        currentDist = ~parametricDistribution.(params.type, params.param1, params.param2);
    });

plotButton = Button(win, Rect(10, 140, 150, 30))
    .states_([["Plot Distribution"]])
    .action_({
        // Create new plot window
        var samples = Array.fill(10000, { currentDist.value });
        var plot = samples.plot("Distribution - " + params.type);
        plot.plotMode = \histogram;
        plot.refresh;
    });

audioButton = Button(win, Rect(10, 180, 70, 30))
    .states_([["Play"]])
    .action_({
        ~playDistribution.(currentDist, 440, 300, 0.05);
    });

stopButton = Button(win, Rect(90, 180, 70, 30))
    .states_([["Stop"]])
    .action_({
        s.freeAll;
    });

// Initialize
params = (
    type: \gaussian,
    param1: 0.15,
    param2: 0.5
);
currentDist = ~parametricDistribution.(params.type, params.param1, params.param2);
)

// ==============================
// NOTES AND OBSERVATIONS
// ==============================

/*
Distribution findings:

1. Gaussian (Normal) distributions with stdDev around 0.15 provide natural-sounding, 
   focused results that avoid extremes.

2. Beta distributions offer the most flexibility for shaping the probability curve, 
   with parameters allowing for skewed distributions that can emphasize specific
   regions of the parameter space.

3. Bimodal distributions are useful for creating contrast between two distinct
   parameter regions, like having droplets predominantly in front and behind 
   the listener.

4. For azimuth, a uniform distribution creates the widest stereo field, while
   a gaussian narrowly focused around center gives a more centered image.

5. For radius, exponential distributions (more values near min radius) create
   a dense sound with occasional distant droplets.

6. For elevation, triangular distributions centered at 0 provide a natural 
   "mostly horizontal with some height variation" characteristic.

Key implementation insights:
- All distributions should be normalized to [0,1] range, then mapped to parameter ranges
- Different parameters benefit from different distribution types
- Interactive control over distribution parameters is essential for sound design
*/
