import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	server: {
		proxy: {
			'/api/ws': {
				target: 'ws://192.168.0.112:12345',
				ws: true,
				rewriteWsOrigin: true
			},
			'/api': 'http://192.168.0.112:12345'
		},
		host: true
	}
});
