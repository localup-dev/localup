export function generateLetterLogo(letter: string, options?: {
    size?: number;
    bgColor?: string;
    textColor?: string;
    fontSize?: number;
    fontFamily?: string;
    paddingX?: number;
    paddingY?: number;
}): string {
    const {
        size = 200,
        bgColor = '#0f0f3d',
        textColor = '#ffffff',
        fontSize = size * 0.6,
        fontFamily = 'Arial Black, Helvetica, sans-serif',
        paddingX = 0,
        paddingY = 0
    } = options ?? {};

    const width = size + paddingX * 2;
    const height = size + paddingY * 2;

    return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}">
    <rect width="${width}" height="${height}" fill="${bgColor}"/>
    <text x="${width / 2}" y="${height / 2 + fontSize / 3}"
          font-family="${fontFamily}"
          font-weight="900"
          font-size="${fontSize}"
          fill="${textColor}"
          text-anchor="middle">${letter.charAt(0)}</text>
  </svg>`;
}

export function generateLogo(name: string, options?: {
    bgColor?: string;
    textColor?: string;
    fontSize?: number;
    fontFamily?: string;
    paddingX?: number;
    paddingY?: number;
}): string {
    const {
        bgColor = '#0f0f3d',
        textColor = '#ffffff',
        fontSize = 36,
        fontFamily = 'Arial Black, Helvetica, sans-serif',
        paddingX = 10,
        paddingY = 10
    } = options ?? {};

    // Calculate dimensions based on text size + padding
    const textWidth = name.length * fontSize * 0.6;
    const width = textWidth + paddingX * 2;
    const height = fontSize + paddingY * 2;

    return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}">
    <rect width="${width}" height="${height}" fill="${bgColor}"/>
    <text x="${width / 2}" y="${height / 2 + fontSize / 3}"
          font-family="${fontFamily}"
          font-weight="900"
          font-size="${fontSize}"
          fill="${textColor}"
          text-anchor="middle">${name}</text>
  </svg>`;
}

// Generate main logo
const logo = generateLogo('localup', {
    bgColor: '#0f0f3d',
    textColor: '#ffffff',
    fontSize: 38,
    fontFamily: 'Inter, Segoe UI, Arial, sans-serif',
    paddingX: 20,
    paddingY: 10
});

// Generate letter logo (for favicon)
const letterLogo = generateLetterLogo('L', {
    size: 200,
    bgColor: '#0f0f3d',
    textColor: '#ffffff',
    fontFamily: 'Inter, Segoe UI, Arial, sans-serif'
});

// Generate smaller favicon versions
const favicon32 = generateLetterLogo('L', {
    size: 32,
    bgColor: '#0f0f3d',
    textColor: '#ffffff',
    fontFamily: 'Inter, Segoe UI, Arial, sans-serif'
});

const favicon16 = generateLetterLogo('L', {
    size: 16,
    bgColor: '#0f0f3d',
    textColor: '#ffffff',
    fontFamily: 'Inter, Segoe UI, Arial, sans-serif'
});

// Write to webapps public directories
const webapps = [
    'webapps/exit-node-portal/public',
    'webapps/dashboard/public'
];

for (const dir of webapps) {
    try {
        await Bun.write(`${dir}/logo.svg`, logo);
        await Bun.write(`${dir}/favicon.svg`, letterLogo);
        console.log(`Generated logos in ${dir}`);
    } catch (e) {
        console.log(`Skipping ${dir} (may not exist)`);
    }
}

// Also write to root for reference
await Bun.write('logo.svg', logo);
await Bun.write('favicon.svg', letterLogo);

console.log('Logo generation complete!');
console.log('Generated files:');
console.log('  - logo.svg (full text logo)');
console.log('  - favicon.svg (letter logo for favicon)');
