const path = require('path');
const HtmlWebpackPlugin = require('html-webpack-plugin');

// Custom plugin to inline JS into HTML
class InlineSourcePlugin {
  apply(compiler) {
    compiler.hooks.compilation.tap('InlineSourcePlugin', (compilation) => {
      HtmlWebpackPlugin.getHooks(compilation).beforeEmit.tapAsync(
        'InlineSourcePlugin',
        (data, cb) => {
          // Get the JS content
          const jsAssets = Object.keys(compilation.assets).filter(asset => 
            asset.endsWith('.js') && !asset.includes('runtime')
          );
          
          if (jsAssets.length > 0) {
            const jsContent = compilation.assets[jsAssets[0]].source();
            
            // Replace script tag with inline script
            data.html = data.html.replace(
              /<script[^>]+src="[^"]*"[^>]*><\/script>/g,
              `<script>${jsContent}</script>`
            );
            
            // Remove the JS file from assets so it's not written to disk
            jsAssets.forEach(asset => {
              delete compilation.assets[asset];
            });
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
      filename: 'index.html',
      inject: 'body',
      scriptLoading: 'blocking',
      minify: false,
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