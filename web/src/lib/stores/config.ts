import { writable, type Writable, get } from 'svelte/store';
import api from '$lib/api';
import type { Config } from '$lib/types';

// 配置 store
export const configStore: Writable<Config | null> = writable<Config | null>(null);

// 加载配置
export async function loadConfig(): Promise<Config | null> {
	try {
		const response = await api.getConfig();
		configStore.set(response.data);
		return response.data;
	} catch (error) {
		console.error('加载配置失败:', error);
		return null;
	}
}

// 获取配置（如果未加载则先加载）
export async function getConfig(): Promise<Config | null> {
	let config = get(configStore);
	if (!config) {
		config = await loadConfig();
	}
	return config;
}

