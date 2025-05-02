// Simply Droplets - Distributions Prototype
// This file implements and tests various distribution models for the Simply Droplets plugin

// ==============================
// UTILITY FUNCTIONS
// ==============================

// Function to plot a distribution
~plotDistribution = {|distribution, numSamples=1000, title="Distribution"|
    var samples;
    var plot;

    samples = Array.fill(numSamples, { distribution.value });
    plot = samples.plot(title);
    // Don't try to set plotMode to histogram as it might not be available in all SC versions
    plot.refresh;
};

// Alternative histogram-like visualization
~plotDistributionHistogram = {|distribution, numSamples=1000, numBins=20, title="Distribution Histogram"|
    var samples;
    var bins;
    var binWidth;
    var maxVal;

    samples = Array.fill(numSamples, { distribution.value });
    bins = Array.fill(numBins, { 0 });
    binWidth = 1.0 / numBins;

    // Count samples in each bin
    samples.do {|sample|
        var binIndex = (sample / binWidth).asInteger.clip(0, numBins-1);
        bins[binIndex] = bins[binIndex] + 1;
    };

    // Normalize bins for better visualization
    maxVal = bins.maxItem;
    bins = bins.collect {|val| val / maxVal };

    // Plot the histogram
    bins.plot(title);
};

// Function to play a distribution as pitch values
~playDistribution = {|distribution, baseFreq=440, spread=200, dur=0.1|
    {
        var trig;
        var freq;
        var sig;

        trig = Impulse.kr(1/dur);
        freq = baseFreq + (distribution.value * spread);
        sig = SinOsc.ar(freq) * EnvGen.kr(Env.perc(0.01, dur), trig);
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
~uniform = {
	1.0.rand
};

// Gaussian (normal) distribution
// Mean = 0.5, standard deviation adjustable
~gaussian = {|stdDev=0.15|
	var u1;
	var u2;
	var r;
	var theta;
	var val;

	u1 = 1.0.rand;
	u2 = 1.0.rand;
	r = (-2 * u1.log).sqrt;
	theta = 2pi * u2;
	val = r * theta.sin;
	// Map from typical [-3,3] range to [0,1] and clip
	(val * stdDev + 0.5).clip(0, 1);
};

// Triangular distribution - higher probability in the middle
~triangular = {
	var a, b;

	a = 1.0.rand;
	b = 1.0.rand;
	(a + b) * 0.5;
};

// Exponential distribution - more values near 0
~exponential = {|lambda=5|
    (1.0 - (-1 * lambda * 1.0.rand).exp).clip(0, 1);
};

// Bimodal distribution - two peaks
~bimodal = {|peak1=0.25, peak2=0.75, mix=0.5, stdDev=0.1|
	var dist;

	dist = if(1.0.rand < mix.value, {
		peak1.value
	}, {
		peak2.value
	});

	// Add gaussian noise
	(dist.value + (0.gaussCurve * stdDev.value)).clip(0, 1);
};

// Beta distribution - flexible shape based on alpha/beta
~beta = {|alpha=2, beta=2|
	var u, v, x, y;

	u = 1.0.rand;
	v = 1.0.rand;
	x = u.pow(1 / alpha.value);
	y = v.pow(1 / beta.value);
	if ((x + y) <= 1, {
		x / (x + y)
	}, {
		~beta.value(alpha, beta)
	});
};

// ==============================
// PARAMETRIC DISTRIBUTION
// ==============================
~parametricDistribution = {|type=\uniform, param1=0.15, param2=0.5|
    case
    { type == \uniform } { ~uniform }
    { type == \gaussian } { { ~gaussian.value(param1) } }
    { type == \triangular } { ~triangular }
    { type == \exponential } { { ~exponential.value(param1 * 10) } }
    { type == \bimodal } { { ~bimodal.value(param1, param2, 0.5, 0.1) } }
    { type == \beta } { { ~beta.value(param1 * 10, param2 * 10) } }
    { ~uniform };  // Default to uniform if unknown type
};
// ==============================
// RADIAL COORDINATE DISTRIBUTIONS
// ==============================

// Radius distribution (distance from center)
~radius = {|minRadius=0, maxRadius=1, distribution|
    var value;

    value = distribution.value;
    minRadius + (value * (maxRadius - minRadius));
};

// Azimuth distribution (horizontal angle 0-2π)
~azimuth = {|minAzimuth=0, maxAzimuth=2pi, distribution|
    var value;

    value = distribution.value;
    minAzimuth + (value * (maxAzimuth - minAzimuth));
};

// Elevation distribution (vertical angle -π/2 to π/2)
~elevation = {|minElevation= -0.5pi, maxElevation=0.5pi, distribution|
    var value;

    value = distribution.value;
    minElevation + (value * (maxElevation - minElevation));
};

// ==============================
// TESTING AND VISUALIZATION
// ==============================

// Start the server for audio testing
s.boot;

// Plot various distributions
(
// Simple distributions
~plotDistribution.value(~uniform, 10000, "Uniform Distribution");
~plotDistribution.value(~gaussian, 10000, "Gaussian Distribution");
~plotDistribution.value(~triangular, 10000, "Triangular Distribution");
~plotDistribution.value(~exponential, 10000, "Exponential Distribution");
~plotDistribution.value(~bimodal, 10000, "Bimodal Distribution");
~plotDistribution.value(~beta, 10000, "Beta Distribution");

// Try the alternative histogram visualization
~plotDistributionHistogram.value(~gaussian, 10000, 30, "Gaussian Distribution Histogram");
~plotDistributionHistogram.value(~bimodal, 10000, 30, "Bimodal Distribution Histogram");
)

// Audio testing of distributions
(
~playDistribution.(~gaussian, 440, 200, 0.1);
)

// ==============================
// INTERACTIVE TESTING
// ==============================

// GUI for testing distributions
(
var win;
var distTypeMenu;
var param1Slider;
var param2Slider;
var plotButton;
var audioButton;
var stopButton;
var currentDist;
var plotView;
var params;

// Create window
win = Window("Distribution Tester", Rect(100, 100, 500, 400)).front;

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
        // Use alternative histogram visualization
        ~plotDistributionHistogram.(currentDist, 10000, 30, "Distribution - " + params.type);
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

// Add explanatory text
TextView(win, Rect(170, 10, 320, 380))
    .string_(
        "Distribution Testing in Simply Droplets\n\n" ++
        "This prototype demonstrates different probability distributions\n" ++
        "for controlling droplet parameters.\n\n" ++
        "Available distribution types:\n" ++
        "• Uniform: Equal probability across the entire range\n" ++
        "• Gaussian: Bell curve centered in the middle\n" ++
        "• Triangular: Linear falloff from center to edges\n" ++
        "• Exponential: More values near minimum\n" ++
        "• Bimodal: Two distinct centers of activity\n" ++
        "• Beta: Versatile distribution with flexible shape\n\n" ++
        "Distribution parameters:\n" ++
        "• Parameter 1: Controls width/spread or shape\n" ++
        "• Parameter 2: Controls position or secondary shaping\n\n" ++
        "Click 'Play' to hear how the distribution affects pitch.\n" ++
        "Click 'Plot Distribution' to see the probability density.\n\n" ++
        "When implemented in droplets, each parameter (position, duration,\n" ++
        "timewarp, radius, azimuth, elevation) can have its own distribution\n" ++
        "type, creating rich and complex textures."
    )
    .editable_(false);
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
