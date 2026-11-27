<?php

declare(strict_types=1);

namespace App\Repository;

use DateTimeImmutable;

/**
 * Refresh 토큰 데이터 접근 계약.
 */
interface RefreshTokenRepositoryInterface
{
    public function store(int $userId, string $tokenHash, DateTimeImmutable $expiresAt): void;

    /**
     * @return array<string, mixed>|null
     */
    public function findByHash(string $tokenHash): ?array;

    /**
     * findByHash 와 같은 조회를 행 잠금(SELECT ... FOR UPDATE)으로 수행한다 — 트랜잭션 안에서만 사용.
     *
     * @return array<string, mixed>|null
     */
    public function lockByHash(string $tokenHash): ?array;

    public function revoke(int $id, DateTimeImmutable $at): void;

    /**
     * 재사용 감지 시 해당 사용자의 유효한 모든 토큰을 무효화한다.
     */
    public function revokeAllForUser(int $userId, DateTimeImmutable $at): void;

    /**
     * 비밀번호 변경·재설정 시 해당 사용자의 모든 토큰을 삭제한다.
     *
     * 폐기(revoked_at)가 아니라 삭제하는 이유: 폐기된 토큰을 다른 기기가 다시 쓰면 재사용 감지가
     * 작동해 비밀번호 변경 후 새로 발급한 세션까지 무효화된다. 삭제된 토큰은 '없는 토큰'으로
     * 거절(INVALID_TOKEN)될 뿐 연쇄 무효화를 일으키지 않는다.
     */
    public function deleteAllForUser(int $userId): void;
}
