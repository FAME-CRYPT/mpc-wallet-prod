#!/usr/bin/env python3

import json
import sys
import statistics
from pathlib import Path
from datetime import datetime
import matplotlib.pyplot as plt
import numpy as np

class BenchmarkAnalyzer:
    def __init__(self, results_file):
        with open(results_file, 'r') as f:
            self.results = json.load(f)

        self.sharedmem = next((r for r in self.results if r['system'] == 'P2pComm'), None)
        self.with_mtls = next((r for r in self.results if r['system'] == 'MtlsComm'), None)

    def compare_metric(self, metric_name, higher_is_better=True):
        """Compare a single metric between the two systems"""
        if not self.sharedmem or not self.with_mtls:
            return None

        val1 = self.sharedmem['metrics'].get(metric_name, 0)
        val2 = self.with_mtls['metrics'].get(metric_name, 0)

        if val1 == 0:
            return None

        diff_percent = ((val2 - val1) / val1) * 100

        if higher_is_better:
            winner = 'mtls-comm' if val2 > val1 else 'p2p-comm'
        else:
            winner = 'mtls-comm' if val2 < val1 else 'p2p-comm'

        return {
            'metric': metric_name,
            'sharedmem': val1,
            'with_mtls': val2,
            'diff_percent': diff_percent,
            'winner': winner
        }

    def generate_comparison_table(self):
        """Generate a comprehensive comparison table"""
        comparisons = []

        # Throughput metrics (higher is better)
        comparisons.append(self.compare_metric('votes_per_second', True))
        comparisons.append(self.compare_metric('messages_per_second', True))
        comparisons.append(self.compare_metric('bytes_per_second', True))

        # Latency metrics (lower is better)
        comparisons.append(self.compare_metric('latency_p50', False))
        comparisons.append(self.compare_metric('latency_p95', False))
        comparisons.append(self.compare_metric('latency_p99', False))
        comparisons.append(self.compare_metric('latency_mean', False))

        # Resource metrics (lower is better)
        comparisons.append(self.compare_metric('cpu_usage_percent', False))
        comparisons.append(self.compare_metric('memory_usage_mb', False))

        # Security metrics (lower is better for overhead)
        comparisons.append(self.compare_metric('tls_handshake_time_us', False))
        comparisons.append(self.compare_metric('cert_validation_time_us', False))
        comparisons.append(self.compare_metric('encryption_overhead_percent', False))

        # Reliability metrics (higher is better)
        comparisons.append(self.compare_metric('delivery_success_rate', True))

        return [c for c in comparisons if c is not None]

    def print_comparison_table(self):
        """Print a formatted comparison table"""
        comparisons = self.generate_comparison_table()

        print("\n" + "="*100)
        print("📊 COMPREHENSIVE BENCHMARK COMPARISON")
        print("="*100)
        print()

        # Header
        print(f"{'Metric':<35} | {'p2p-comm':>15} | {'mtls-comm':>15} | {'Diff %':>10} | {'Winner':<20}")
        print("-"*100)

        # Group by category
        categories = {
            'Throughput': ['votes_per_second', 'messages_per_second', 'bytes_per_second'],
            'Latency': ['latency_p50', 'latency_p95', 'latency_p99', 'latency_mean'],
            'Resource Usage': ['cpu_usage_percent', 'memory_usage_mb'],
            'Security': ['tls_handshake_time_us', 'cert_validation_time_us', 'encryption_overhead_percent'],
            'Reliability': ['delivery_success_rate']
        }

        for category, metrics in categories.items():
            print(f"\n{category.upper()}")
            print("-"*100)

            for comp in comparisons:
                if comp['metric'] in metrics:
                    symbol = '↑' if comp['diff_percent'] > 0 else '↓'
                    winner_mark = '✓' if comp['winner'] == 'mtls-comm' else ''

                    print(f"{comp['metric']:<35} | {comp['sharedmem']:>15.2f} | {comp['with_mtls']:>15.2f} | {symbol}{abs(comp['diff_percent']):>9.1f}% | {comp['winner']:<20} {winner_mark}")

        print("\n" + "="*100)

    def calculate_overall_winner(self):
        """Calculate overall winner based on weighted scores"""
        comparisons = self.generate_comparison_table()

        # Weights for different metric categories
        weights = {
            'votes_per_second': 10,
            'messages_per_second': 5,
            'latency_p50': 10,
            'latency_p95': 8,
            'latency_p99': 6,
            'cpu_usage_percent': 7,
            'memory_usage_mb': 6,
            'tls_handshake_time_us': 4,
            'delivery_success_rate': 9,
        }

        sharedmem_score = 0
        with_mtls_score = 0

        for comp in comparisons:
            metric = comp['metric']
            weight = weights.get(metric, 1)

            if comp['winner'] == 'p2p-comm':
                sharedmem_score += weight
            else:
                with_mtls_score += weight

        total_score = sharedmem_score + with_mtls_score

        return {
            'sharedmem': {
                'score': sharedmem_score,
                'percentage': (sharedmem_score / total_score) * 100 if total_score > 0 else 0
            },
            'with_mtls': {
                'score': with_mtls_score,
                'percentage': (with_mtls_score / total_score) * 100 if total_score > 0 else 0
            },
            'winner': 'mtls-comm' if with_mtls_score > sharedmem_score else 'p2p-comm'
        }

    def print_overall_summary(self):
        """Print overall summary"""
        scores = self.calculate_overall_winner()

        print("\n" + "="*100)
        print("🏆 OVERALL WINNER")
        print("="*100)
        print()

        print(f"p2p-comm (libp2p):       {scores['sharedmem']['score']:>3} points ({scores['sharedmem']['percentage']:.1f}%)")
        print(f"mtls-comm (pure mTLS):    {scores['with_mtls']['score']:>3} points ({scores['with_mtls']['percentage']:.1f}%)")
        print()
        print(f"🎉 Winner: {scores['winner'].upper()}")
        print()
        print("="*100)

    def plot_comparison_charts(self, output_dir='./charts'):
        """Generate comparison charts"""
        Path(output_dir).mkdir(exist_ok=True)

        comparisons = self.generate_comparison_table()

        # 1. Throughput comparison
        throughput_metrics = ['votes_per_second', 'messages_per_second']
        throughput_data = [(c['metric'], c['sharedmem'], c['with_mtls'])
                          for c in comparisons if c['metric'] in throughput_metrics]

        if throughput_data:
            self._plot_bar_comparison(
                throughput_data,
                'Throughput Comparison',
                'Metric',
                'Value',
                f'{output_dir}/throughput_comparison.png'
            )

        # 2. Latency comparison
        latency_metrics = ['latency_p50', 'latency_p95', 'latency_p99']
        latency_data = [(c['metric'], c['sharedmem'], c['with_mtls'])
                       for c in comparisons if c['metric'] in latency_metrics]

        if latency_data:
            self._plot_bar_comparison(
                latency_data,
                'Latency Comparison (μs)',
                'Percentile',
                'Latency (μs)',
                f'{output_dir}/latency_comparison.png'
            )

        # 3. Resource usage comparison
        resource_metrics = ['cpu_usage_percent', 'memory_usage_mb']
        resource_data = [(c['metric'], c['sharedmem'], c['with_mtls'])
                        for c in comparisons if c['metric'] in resource_metrics]

        if resource_data:
            self._plot_bar_comparison(
                resource_data,
                'Resource Usage Comparison',
                'Metric',
                'Value',
                f'{output_dir}/resource_comparison.png'
            )

        print(f"\n📊 Charts saved to: {output_dir}/")

    def _plot_bar_comparison(self, data, title, xlabel, ylabel, output_path):
        """Helper to plot bar comparison charts"""
        if not data:
            return

        labels = [d[0] for d in data]
        sharedmem_values = [d[1] for d in data]
        with_mtls_values = [d[2] for d in data]

        x = np.arange(len(labels))
        width = 0.35

        fig, ax = plt.subplots(figsize=(12, 6))
        bars1 = ax.bar(x - width/2, sharedmem_values, width, label='p2p-comm (libp2p)', color='#3498db')
        bars2 = ax.bar(x + width/2, with_mtls_values, width, label='mtls-comm (pure mTLS)', color='#2ecc71')

        ax.set_xlabel(xlabel)
        ax.set_ylabel(ylabel)
        ax.set_title(title)
        ax.set_xticks(x)
        ax.set_xticklabels(labels, rotation=45, ha='right')
        ax.legend()

        # Add value labels on bars
        for bars in [bars1, bars2]:
            for bar in bars:
                height = bar.get_height()
                ax.text(bar.get_x() + bar.get_width()/2., height,
                       f'{height:.1f}',
                       ha='center', va='bottom', fontsize=8)

        plt.tight_layout()
        plt.savefig(output_path, dpi=300, bbox_inches='tight')
        plt.close()

    def export_to_csv(self, output_file='benchmark_comparison.csv'):
        """Export comparison to CSV"""
        comparisons = self.generate_comparison_table()

        with open(output_file, 'w') as f:
            f.write('Metric,p2p-comm,mtls-comm,Difference %,Winner\n')
            for comp in comparisons:
                f.write(f"{comp['metric']},{comp['sharedmem']:.2f},{comp['with_mtls']:.2f},{comp['diff_percent']:.2f},{comp['winner']}\n")

        print(f"\n📄 CSV exported to: {output_file}")

def main():
    if len(sys.argv) < 2:
        print("Usage: python analyze_results.py <results.json>")
        sys.exit(1)

    results_file = sys.argv[1]

    if not Path(results_file).exists():
        print(f"Error: File not found: {results_file}")
        sys.exit(1)

    analyzer = BenchmarkAnalyzer(results_file)

    # Print comparison table
    analyzer.print_comparison_table()

    # Print overall summary
    analyzer.print_overall_summary()

    # Generate charts
    try:
        analyzer.plot_comparison_charts()
    except Exception as e:
        print(f"\n⚠️  Warning: Could not generate charts: {e}")
        print("    Install matplotlib: pip install matplotlib numpy")

    # Export to CSV
    analyzer.export_to_csv()

    print("\n✅ Analysis complete!")

if __name__ == '__main__':
    main()
