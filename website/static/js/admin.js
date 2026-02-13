// Velocty Admin — Minimal vanilla JS

(function() {
    'use strict';

    // Toast notifications
    function showToast(message, type) {
        const toast = document.createElement('div');
        toast.className = 'alert alert-' + (type || 'success');
        toast.textContent = message;
        toast.style.cssText = 'position:fixed;top:16px;right:16px;z-index:9999;min-width:250px;animation:fadeIn 200ms';
        document.body.appendChild(toast);
        setTimeout(() => toast.remove(), 3000);
    }

    // Sidebar hover expand — handled by CSS, but we add touch support
    const sidebar = document.getElementById('sidebar');
    if (sidebar) {
        sidebar.addEventListener('touchstart', function() {
            this.classList.toggle('expanded');
        });
    }

    // Confirm delete forms
    document.querySelectorAll('form[onsubmit]').forEach(form => {
        // Already handled inline
    });

    // Dashboard chart loading (placeholder until D3 charts are built)
    if (typeof d3 !== 'undefined' && document.getElementById('chart-sankey')) {
        loadDashboardCharts();
    }

    async function loadDashboardCharts() {
        try {
            const [overview, referrers, topPortfolio, calendar] = await Promise.all([
                fetch('/admin/api/stats/overview').then(r => r.json()),
                fetch('/admin/api/stats/top-referrers').then(r => r.json()),
                fetch('/admin/api/stats/top-portfolio').then(r => r.json()),
                fetch('/admin/api/stats/calendar').then(r => r.json()),
            ]);

            // Update stat cards if overview data exists
            if (overview && overview.total_views !== undefined) {
                const statNumbers = document.querySelectorAll('.stat-number');
                // Stats are rendered server-side, but we could update live here
            }

            // Render referrers bar chart
            if (referrers && referrers.length > 0) {
                renderHorizontalBars('#chart-referrers', referrers);
            }

            // Render top portfolio radial
            if (topPortfolio && topPortfolio.length > 0) {
                renderHorizontalBars('#chart-top-portfolio', topPortfolio);
            }

            // Render calendar heatmap
            if (calendar && calendar.length > 0) {
                renderCalendarHeatmap('#chart-calendar', calendar);
            }

        } catch (e) {
            console.log('Dashboard charts: waiting for data', e);
        }
    }

    function renderHorizontalBars(selector, data) {
        const container = document.querySelector(selector);
        if (!container || !data.length) return;
        container.innerHTML = '';

        const width = container.clientWidth || 300;
        const barHeight = 28;
        const height = data.length * barHeight + 20;
        const maxCount = Math.max(...data.map(d => d.count));

        const svg = d3.select(selector)
            .append('svg')
            .attr('width', width)
            .attr('height', height);

        const barWidth = width - 140;

        svg.selectAll('g')
            .data(data)
            .join('g')
            .attr('transform', (d, i) => `translate(0, ${i * barHeight})`)
            .each(function(d) {
                const g = d3.select(this);
                g.append('text')
                    .attr('x', 0)
                    .attr('y', barHeight / 2 + 4)
                    .attr('fill', '#9ca3af')
                    .attr('font-size', '12px')
                    .text(d.label.replace(/^\/portfolio\//, '').replace(/^\/blog\//, ''));

                g.append('rect')
                    .attr('x', 120)
                    .attr('y', 4)
                    .attr('width', 0)
                    .attr('height', barHeight - 8)
                    .attr('fill', '#2dd4bf')
                    .attr('rx', 3)
                    .transition()
                    .duration(600)
                    .attr('width', (d.count / maxCount) * barWidth);

                g.append('text')
                    .attr('x', 120 + (d.count / maxCount) * barWidth + 8)
                    .attr('y', barHeight / 2 + 4)
                    .attr('fill', '#6b7280')
                    .attr('font-size', '11px')
                    .text(d.count);
            });
    }

    function renderCalendarHeatmap(selector, data) {
        const container = document.querySelector(selector);
        if (!container || !data.length) return;
        container.innerHTML = '';

        const cellSize = 14;
        const width = container.clientWidth || 700;
        const height = cellSize * 7 + 40;
        const maxCount = Math.max(...data.map(d => d.count));

        const colorScale = d3.scaleLinear()
            .domain([0, maxCount])
            .range(['#0a3d2e', '#2dd4bf']);

        const svg = d3.select(selector)
            .append('svg')
            .attr('width', width)
            .attr('height', height);

        const dateMap = new Map(data.map(d => [d.date, d.count]));

        svg.selectAll('rect')
            .data(data)
            .join('rect')
            .attr('x', (d, i) => Math.floor(i / 7) * (cellSize + 2))
            .attr('y', (d, i) => (i % 7) * (cellSize + 2))
            .attr('width', cellSize)
            .attr('height', cellSize)
            .attr('rx', 2)
            .attr('fill', d => colorScale(d.count))
            .append('title')
            .text(d => `${d.date}: ${d.count} views`);
    }

})();
