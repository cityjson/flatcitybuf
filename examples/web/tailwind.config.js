/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      // CityJSON brand palette, taken from the logo (cityjson.org).
      colors: {
        cj: {
          purple: '#6449d6', // primary actions
          'purple-dark': '#5238c0', // hover
          gold: '#f5aa00', // accent / selected states
          'gold-soft': '#fff4dc', // gold tint for selected backgrounds
          green: '#7ca12b', // secondary / success
          charcoal: '#615e5c', // body text + headings
          'charcoal-soft': '#8a8783', // muted text
        },
      },
    },
  },
  plugins: [],
}
