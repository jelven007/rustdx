#!/bin/bash
# rustdx-complete 网络连接诊断脚本
#
# 用于快速诊断 rustdx-complete 的网络连接问题

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_header() {
    echo -e "${BLUE}============================================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}============================================================${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

# 通达信服务器列表
SERVERS=(
    "115.238.56.198:7709"
    "114.80.149.19:7709"
    "114.80.149.22:7709"
    "39.100.68.59:7709"
    "115.238.90.165:7709"
    "218.108.47.69:7709"
    "60.12.136.250:7709"
)

# 主函数
main() {
    print_header "rustdx-complete 网络连接诊断"
    echo ""

    # 1. 检查网络连接
    print_header "1. 检查网络连接"
    echo ""

    if ping -c 1 8.8.8.8 &> /dev/null; then
        print_success "互联网连接正常"
    else
        print_error "无法连接到互联网"
        exit 1
    fi
    echo ""

    # 2. 测试通达信服务器连接
    print_header "2. 测试通达信服务器连接"
    echo ""
    print_info "测试连接到通达信公共服务器..."
    echo ""

    successful_count=0
    failed_count=0

    for server in "${SERVERS[@]}"; do
        host=$(echo $server | cut -d: -f1)
        port=$(echo $server | cut -d: -f2)

        echo -n "测试 $server ... "

        if timeout 3 nc -zv "$host" "$port" 2>&1 | grep -q "succeeded"; then
            print_success "连接成功"
            ((successful_count++))
        else
            print_error "连接失败"
            ((failed_count++))
        fi
    done

    echo ""
    print_info "结果: $successful_count 个可用, $failed_count 个失败"
    echo ""

    # 3. 测试 Rust 连接
    print_header "3. 测试 Rust TCP 连接"
    echo ""

    if command -v cargo &> /dev/null; then
        print_info "运行 Rust 连接测试..."
        echo ""

        if cargo test check_all_stock_ips -- --nocapture 2>&1 | tee /tmp/rust_test.log; then
            print_success "Rust 连接测试通过"
        else
            print_error "Rust 连接测试失败"
            print_info "查看详细日志: /tmp/rust_test.log"
        fi
    else
        print_warning "未找到 cargo，跳过 Rust 测试"
    fi
    echo ""

    # 4. 检查防火墙
    print_header "4. 检查防火墙规则"
    echo ""

    if command -v ufw &> /dev/null; then
        if ufw status | grep -q "Status: active"; then
            print_warning "UFW 防火墙已启用"
            print_info "请确保允许 TCP 出站连接"
        else
            print_success "UFW 防火墙未启用或已允许出站"
        fi
    elif command -v firewall-cmd &> /dev/null; then
        if firewall-cmd --state &> /dev/null; then
            print_warning "firewalld 防火墙已启用"
            print_info "请确保允许 TCP 出站连接"
        else
            print_success "firewalld 防火墙未运行"
        fi
    else
        print_info "未检测到常见防火墙"
    fi
    echo ""

    # 5. 常见问题检查
    print_header "5. 常见问题检查"
    echo ""

    # 检查是否误解了本地端口
    print_info "澄清: rustdx-complete 不需要本地通达信服务"
    print_success "我们连接的是远程服务器，不是本地 2222 端口"
    echo ""

    # 检查代理设置
    if [ -n "$http_proxy" ] || [ -n "$https_proxy" ]; then
        print_warning "检测到代理设置"
        print_info "HTTP_PROXY=$http_proxy"
        print_info "HTTPS_PROXY=$https_proxy"
    else
        print_success "未设置代理"
    fi
    echo ""

    # 6. 推荐服务器
    print_header "6. 推荐配置"
    echo ""

    if [ $successful_count -gt 0 ]; then
        print_success "找到可用服务器"
        echo ""
        print_info "推荐使用以下代码:"
        echo ""
        echo -e "${GREEN}use rustdx_complete::tcp::{Tcp, Tdx};${NC}"
        echo -e "${GREEN}use std::net::SocketAddr;${NC}"
        echo ""
        echo -e "${GREEN}fn main() -> Result<(), Box<dyn std::error::Error>> {${NC}"
        echo -e "${GREEN}    // 使用第一个可用服务器${NC}"
        echo -e "${GREEN}    let mut tcp = Tcp::new()?;${NC}"
        echo -e "${GREEN}    // 或者指定服务器:${NC}"
        echo -e "${GREEN}    let addr: SocketAddr = \"${SERVERS[0]}\".parse()?;${NC}"
        echo -e "${GREEN}    let mut tcp = Tcp::new_with_ip(&addr)?;${NC}"
        echo -e "${GREEN}    Ok(())${NC}"
        echo -e "${GREEN}}${NC}"
        echo ""
    else
        print_error "未找到可用服务器"
        echo ""
        print_info "可能的原因:"
        print_info "1. 网络连接问题"
        print_info "2. 防火墙阻止 TCP 连接"
        print_info "3. 需要配置代理"
        echo ""
        print_info "建议:"
        print_info "1. 检查网络连接: ping 115.238.56.198"
        print_info "2. 检查防火墙设置"
        print_info "3. 如果在公司网络，联系网络管理员"
    fi
    echo ""

    # 7. 总结
    print_header "诊断总结"
    echo ""

    if [ $successful_count -ge 3 ]; then
        print_success "网络连接正常，rustdx-complete 应该可以正常使用"
        print_info "可用服务器数量: $successful_count"
    elif [ $successful_count -gt 0 ]; then
        print_warning "部分服务器可用，rustdx-complete 应该可以使用"
        print_info "可用服务器数量: $successful_count"
    else
        print_error "无法连接到任何服务器"
        print_info "请检查网络设置和防火墙配置"
    fi
    echo ""

    print_info "详细文档: NETWORK_CONNECTION_GUIDE.md"
    print_info "报告问题: https://github.com/jelven007/rustdx/issues"
    echo ""
}

# 运行诊断
main
