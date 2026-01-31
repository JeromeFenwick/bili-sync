<script lang="ts">
	import { Button } from '$lib/components/ui/button/index.js';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card/index.js';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import SubscriptionDialog from './subscription-dialog.svelte';
	import UserIcon from '@lucide/svelte/icons/user';
	import VideoIcon from '@lucide/svelte/icons/video';
	import FolderIcon from '@lucide/svelte/icons/folder';
	import HeartIcon from '@lucide/svelte/icons/heart';
	import CheckIcon from '@lucide/svelte/icons/check';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import XIcon from '@lucide/svelte/icons/x';
import type { Followed, VideoSourcesResponse, VideoSource } from '$lib/types';
import api from '$lib/api';
import { goto } from '$app/navigation';
import { toast } from 'svelte-sonner';
import { onMount } from 'svelte';

export let item: Followed;
export let onSubscriptionSuccess: (() => void) | null = null;

let dialogOpen = false;
let videoSources: VideoSourcesResponse | null = null;
let cachedCount: number | null = null;

	function getIcon() {
		switch (item.type) {
			case 'favorite':
				return HeartIcon;
			case 'collection':
				return FolderIcon;
			case 'upper':
				return UserIcon;
			default:
				return VideoIcon;
		}
	}

	function getTypeLabel() {
		switch (item.type) {
			case 'favorite':
				return '收藏夹';
			case 'collection':
				return '合集';
			case 'upper':
				return 'UP 主';
			default:
				return '';
		}
	}

	function getTitle(): string {
		switch (item.type) {
			case 'favorite':
			case 'collection':
				return item.title;
			case 'upper':
				return item.uname;
			default:
				return '';
		}
	}

	function getSubtitle(): string {
		switch (item.type) {
			case 'favorite':
			case 'collection':
				return `UID：${item.mid}`;
			default:
				return '';
		}
	}

	function getDescription(): string {
		switch (item.type) {
			case 'upper':
				return item.sign || '';
			default:
				return '';
		}
	}

	function isDisabled(): boolean {
		switch (item.type) {
			case 'collection':
			case 'upper':
			case 'favorite':
				return item.invalid;
			default:
				return false;
		}
	}

	function getDisabledReason(): string {
		switch (item.type) {
			case 'collection':
				return '已失效';
			case 'upper':
				return '账号已注销';
			default:
				return '';
		}
	}

	function getCount(): number | null {
		switch (item.type) {
			case 'favorite':
			case 'collection':
				return item.media_count;
			default:
				return null;
		}
	}

	function getAvatarUrl(): string {
		switch (item.type) {
			case 'upper':
				return item.face;
			default:
				return '';
		}
	}

	function handleSubscribe() {
		if (!disabled) {
			dialogOpen = true;
		}
	}

	function handleSubscriptionSuccess() {
		// 更新本地状态
		item.subscribed = true;
		if (onSubscriptionSuccess) {
			onSubscriptionSuccess();
		}
	}

	async function ensureVideoSources() {
		if (!videoSources) {
			const resp = await api.getVideoSources();
			videoSources = resp.data;
		}
	}

	async function resolveVideoSource():
		Promise<{ paramKey: 'favorite' | 'collection' | 'submission'; id: number } | null> {
		await ensureVideoSources();
		if (!videoSources) {
			return null;
		}

		let list: VideoSource[] | undefined;
		let paramKey: 'favorite' | 'collection' | 'submission';
		let matchName: string;

		if (item.type === 'favorite') {
			list = videoSources.favorite;
			paramKey = 'favorite';
			matchName = getTitle();
		} else if (item.type === 'collection') {
			list = videoSources.collection;
			paramKey = 'collection';
			matchName = getTitle();
		} else if (item.type === 'upper') {
			list = videoSources.submission;
			paramKey = 'submission';
			matchName = item.uname;
		} else {
			return null;
		}

		// 先尝试精确匹配名称，若失败，对于合集再尝试模糊匹配，
		// 兼容“我追的合集 / 收藏夹”和“视频源”中名称存在细微差异的情况。
		let source = list.find((s) => s.name === matchName);
		if (!source && item.type === 'collection') {
			source = list.find(
				(s) => s.name.includes(matchName) || matchName.includes(s.name)
			);
		}
		if (!source) {
			return null;
		}

		return { paramKey, id: source.id };
	}

	onMount(async () => {
		// 收藏夹 / 合集 / UP 投稿：尝试预取“已缓存数量”
		if (item.type === 'favorite' || item.type === 'collection' || item.type === 'upper') {
			try {
				const resolved = await resolveVideoSource();
				if (!resolved) return;
				const { paramKey, id } = resolved;
				const params: Record<string, string | number> = {
					page: 0,
					page_size: 1,
					status_filter: 'succeeded'
				};
				params[paramKey] = id;
				const res = await api.getVideos(params);
				cachedCount = res.data.total_count;
			} catch {
				cachedCount = null;
			}
		}
	});

	async function handleDrilldown() {
		try {
			const resolved = await resolveVideoSource();
			if (!resolved) {
				toast.info('尚未为该条目创建对应的视频源或尚未启用');
				return;
			}
			const { paramKey, id } = resolved;

			const params = new URLSearchParams();
			params.set(paramKey, String(id));
			goto(`/videos?${params.toString()}`);
		} catch (error) {
			console.error('查看视频失败：', error);
			toast.error('查看视频失败', {
				description: '获取视频源信息时出错，请稍后再试'
			});
		}
	}

	const Icon = getIcon();
	const typeLabel = getTypeLabel();
	const title = getTitle();
	const subtitle = getSubtitle();
	const description = getDescription();
	const count = getCount();
	const avatarUrl = getAvatarUrl();
	const subscribed = item.subscribed;
	const disabled = isDisabled();
	const disabledReason = getDisabledReason();
</script>

<Card
	class="hover:shadow-primary/5 border-border/50 group flex h-[200px] flex-col transition-all hover:shadow-lg {disabled
		? 'opacity-60'
		: ''}"
>
	<CardHeader class="flex-shrink-0">
		<div class="flex items-start gap-3">
			<!-- 头像或图标 - 简化设计 -->
			<div
				class="bg-accent/50 flex h-10 w-10 shrink-0 items-center justify-center rounded-full {disabled
					? 'opacity-50'
					: ''}"
			>
				{#if avatarUrl && item.type === 'upper'}
					<img
						src={avatarUrl}
						alt={title}
						class="h-full w-full rounded-full object-cover {disabled ? 'grayscale' : ''}"
						loading="lazy"
					/>
				{:else}
					<Icon class="text-muted-foreground h-5 w-5" />
				{/if}
			</div>

			<!-- 内容区域 -->
			<div class="min-w-0 flex-1 space-y-2">
				<div class="flex items-start justify-between gap-2">
					<CardTitle
						class="line-clamp-2 text-sm leading-relaxed font-medium {disabled
							? 'text-muted-foreground line-through'
							: ''}"
						{title}
					>
						{title}
					</CardTitle>

					<!-- 状态标记 -->
					{#if disabled}
						<Badge variant="destructive" class="shrink-0 text-xs">不可用</Badge>
					{:else}
						<Badge variant="secondary" class="shrink-0 text-xs">
							{subscribed ? '已订阅' : typeLabel}
						</Badge>
					{/if}
				</div>

				<!-- 副标题和描述 -->
				{#if subtitle && !disabled}
					<div class="text-muted-foreground flex items-center gap-1 text-sm">
						<UserIcon class="h-3 w-3 shrink-0" />
						<span class="truncate" title={subtitle}>{subtitle}</span>
					</div>
				{/if}

				<!-- 计数信息 -->
				{#if !disabled && (count !== null || cachedCount !== null)}
					<div class="text-muted-foreground flex items-center gap-1 text-sm">
						<VideoIcon class="h-3 w-3 shrink-0" />
						{#if count !== null && cachedCount !== null}
							<span class="truncate">视频数：{cachedCount}/{count}</span>
						{:else if cachedCount !== null}
							<span class="truncate">已缓存：{cachedCount}</span>
						{:else}
							<span class="truncate">视频数：{count}</span>
						{/if}
					</div>
				{/if}

				<!-- 描述信息 -->
				{#if description && !disabled}
					<p class="text-muted-foreground line-clamp-1 text-sm" title={description}>
						{description}
					</p>
				{/if}
			</div>
		</div>
	</CardHeader>

	<!-- 底部按钮区域 -->
	<CardContent class="flex min-w-0 flex-1 flex-col justify-end">
		<div class="flex justify-end">
			{#if disabled}
				<Button
					size="sm"
					variant="outline"
					disabled
					class="h-8 cursor-not-allowed text-xs opacity-50"
				>
					<XIcon class="mr-1 h-3 w-3" />
					{disabledReason}
				</Button>
			{:else if subscribed}
				<div class="flex items-center gap-2">
					<Button
						size="sm"
						variant="secondary"
						onclick={handleDrilldown}
						class="h-8 cursor-pointer text-xs font-medium"
					>
						查看视频
					</Button>
					<Button size="sm" variant="outline" disabled class="h-8 cursor-not-allowed text-xs">
						<CheckIcon class="mr-1 h-3 w-3" />
						已订阅
					</Button>
				</div>
			{:else}
				<Button
					size="sm"
					variant="outline"
					onclick={handleSubscribe}
					class="h-8 cursor-pointer text-xs font-medium"
				>
					<PlusIcon class="mr-1 h-3 w-3" />
					订阅
				</Button>
			{/if}
		</div>
	</CardContent>
</Card>

<!-- 订阅对话框 -->
<SubscriptionDialog bind:open={dialogOpen} {item} onSuccess={handleSubscriptionSuccess} />
