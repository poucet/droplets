const path = require('path');
const HtmlWebpackPlugin = require('html-webpack-plugin');

class InlineSourcePlugin {
  apply(compiler) {
    compiler.hooks.compilation.tap('InlineSourcePlugin', (compilation) => {
      HtmlWebpackPlugin.getHooks(compilation).beforeEmit.tapAsync(
        'InlineSourcePlugin',
        (data, cb) => {
          // Get the generated JS content
          const jsFile = Object.keys(compilation.assets).find(name => name.endsWith('.js'));
          if (jsFile) {
            const jsContent = compilation.assets[jsFile].source();
            
            // Replace script tag with inline script
            data.html = data.html.replace(
              /<script[^>]*src="[^"]*"[^>]*><\/script>/gi,
              `<script>${jsContent}</script>`
            );
            
            // Remove the JS file from assets since it's now inline
            delete compilation.assets[jsFile];
          }
          
          cb(null, data);
        }
      );
    });
  }
}

module.exports = {
  entry: './src/index.tsx',
  output: {
    path: path.resolve(__dirname, 'dist'),
    filename: 'bundle.js',
    clean: true,
  },
  module: {
    rules: [
      {
        test: /\.(ts|tsx)$/,
        exclude: /node_modules/,
        use: {
          loader: 'babel-loader',
          options: {
            presets: [
              '@babel/preset-env',
              '@babel/preset-react',
              '@babel/preset-typescript',
            ],
          },
        },
      },
      {
        test: /\.css$/,
        use: ['style-loader', 'css-loader'],
      },
    ],
  },
  resolve: {
    extensions: ['.tsx', '.ts', '.js'],
  },
  plugins: [
    new HtmlWebpackPlugin({
      template: './public/index.html',
      inject: true,
    }),
    new InlineSourcePlugin(),
  ],
  devServer: {
    static: {
      directory: path.join(__dirname, 'dist'),
    },
    compress: true,
    port: 3000,
  },
};