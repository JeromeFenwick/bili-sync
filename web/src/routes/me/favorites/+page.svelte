<script lang="ts">
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';

	import SubscriptionCard from '$lib/components/subscription-card.svelte';
	import Pagination from '$lib/components/pagination.svelte';
	import { setBreadcrumb } from '$lib/stores/breadcrumb';

	import api from '$lib/api';
	import type { Followed, ApiError } from '$lib/types';
	import { getFollowedKey } from '$lib/utils';

	let allFavorites: Followed[] = [];
	let favorites: Followed[] = [];
	let currentPage = 0;
	let loading = false;

	const pageSize = 25;

	async function loadFavorites() {
		loading = true;
		try {
			const response = await api.getCreatedFavorites();
			allFavorites = response.data.favorites;
			updateCurrentPageData();
		} catch (error) {
			console.error('加载收藏夹失败：', error);
			toast.error('加载收藏夹失败', {
				description: (error as ApiError).message
			});
		} finally {
			loading = false;
		}
	}

	function updateCurrentPageData() {
		const start = currentPage * pageSize;
		const end = start + pageSize;
		favorites = allFavorites.slice(start, end);
	}

	function handleSubscriptionSuccess() {
		// 重新加载数据以获取最新状态
		loadFavorites();
	}

	async function handlePageChange(page: number) {
		currentPage = page;
		updateCurrentPageData();
	}

	$: totalPages = Math.ceil(allFavorites.length / pageSize);

	onMount(async () => {
		setBreadcrumb([{ label: '我创建的收藏夹' }]);

		await loadFavorites();
	});
</script>

<svelte:head>
	<title>我创建的收藏夹 - Bili Sync</title>
</svelte:head>

<div>
	<div class="mb-6 flex items-center justify-between">
		<div class="flex items-center gap-6">
			{#if !loading}
				<div class="text-sm font-medium">
					共 {allFavorites.length} 个收藏夹
				</div>
				<div class="text-sm font-medium">
					当前第 {currentPage + 1} / {totalPages} 页
				</div>
			{/if}
		</div>
	</div>

	{#if loading}
		<div class="flex items-center justify-center py-12">
			<div class="text-muted-foreground">加载中...</div>
		</div>
	{:else if favorites.length > 0}
		<div
			style="display: grid; grid-template-columns: repeat(5, 1fr); gap: 16px; width: 100%;"
		>
			{#each favorites as favorite (getFollowedKey(favorite))}
				<SubscriptionCard item={favorite} onSubscriptionSuccess={handleSubscriptionSuccess} />
			{/each}
		</div>

		<!-- 分页组件 -->
		{#if totalPages > 1}
			<Pagination {currentPage} {totalPages} onPageChange={handlePageChange} />
		{/if}
	{:else}
		<div class="flex items-center justify-center py-12">
			<div class="space-y-2 text-center">
				<p class="text-muted-foreground">暂无收藏夹数据</p>
				<p class="text-muted-foreground text-sm">请先在 B 站创建收藏夹，或检查账号配置</p>
			</div>
		</div>
	{/if}
</div>
