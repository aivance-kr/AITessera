<?php

declare(strict_types=1);

namespace Tests\Unit;

use App\Console\CreateOperator;
use App\Domain\JwtAlgorithm;
use App\Domain\UserRole;
use App\Exception\ValidationException;
use App\Repository\EmailVerificationRepositoryInterface;
use App\Repository\RefreshTokenRepositoryInterface;
use App\Repository\UserRepositoryInterface;
use App\Service\UserService;
use App\Support\Config;
use App\Support\ConnectionInterface;
use App\Support\Jwks;
use App\Support\JwtIssuer;
use App\Support\Queue\QueueInterface;
use DateTimeImmutable;
use Lcobucci\JWT\Configuration;
use Lcobucci\JWT\Signer\Hmac\Sha256;
use Lcobucci\JWT\Signer\Key\InMemory;
use PHPUnit\Framework\TestCase;
use Tests\Support\FixedClock;

final class CreateOperatorTest extends TestCase
{
    public function testParseOptionsReadsLongOptions(): void
    {
        $options = CreateOperator::parseOptions([
            'bin/console',
            'operator:create',
            '--email=ops@aivance.test',
            '--name=운영자',
            '--affiliation=aivance',
            '--contact=010-0000-0000',
            '--company=AIvance',
            'stray-argument',
        ]);

        self::assertSame([
            'email' => 'ops@aivance.test',
            'name' => '운영자',
            'affiliation' => 'aivance',
            'contact' => '010-0000-0000',
            'company' => 'AIvance',
        ], $options);
    }

    public function testRunCreatesVerifiedOperatorFromOptions(): void
    {
        $users = $this->createMock(UserRepositoryInterface::class);
        $users->method('emailExists')->with('ops@aivance.test', 'aivance')->willReturn(false);
        $users->expects(self::once())->method('create')
            ->willReturnCallback(static function (mixed ...$args): int {
                self::assertSame('ops@aivance.test', $args[0]);
                self::assertSame('aivance', $args[2]);
                self::assertNull($args[5]); // company 미지정
                self::assertSame(UserRole::Operator, $args[8]);

                return 7;
            });
        $users->expects(self::once())->method('markEmailVerified')->with(7);

        $message = $this->command($users)->run($this->options(), 'Secur3!pass');

        self::assertStringContainsString('운영자 계정 생성 완료', $message);
        self::assertStringContainsString('id=7', $message);
        // 비밀번호는 출력에 절대 포함하지 않는다.
        self::assertStringNotContainsString('Secur3!pass', $message);
    }

    public function testRunSkipsWhenEmailAlreadyExists(): void
    {
        $users = $this->createMock(UserRepositoryInterface::class);
        $users->method('emailExists')->willReturn(true);
        $users->expects(self::never())->method('create');

        $message = $this->command($users)->run($this->options(), 'Secur3!pass');

        self::assertStringContainsString('건너뜁니다', $message);
    }

    public function testRunRejectsWeakPassword(): void
    {
        $users = $this->createMock(UserRepositoryInterface::class);
        $users->expects(self::never())->method('create');

        $this->expectException(ValidationException::class);
        $this->command($users)->run($this->options(), 'short');
    }

    public function testRunRejectsMissingRequiredOption(): void
    {
        $users = $this->createMock(UserRepositoryInterface::class);
        $users->expects(self::never())->method('create');

        $options = $this->options();
        unset($options['contact']);

        $this->expectException(ValidationException::class);
        $this->command($users)->run($options, 'Secur3!pass');
    }

    /**
     * @return array<string, string>
     */
    private function options(): array
    {
        return [
            'email' => 'ops@aivance.test',
            'name' => '운영자',
            'affiliation' => 'aivance',
            'contact' => '010-0000-0000',
        ];
    }

    private function command(UserRepositoryInterface $users): CreateOperator
    {
        $clock = new FixedClock(new DateTimeImmutable('2026-11-27 12:00:00'));
        $config = new Config(
            appEnv: 'testing',
            appDebug: true,
            dbHost: '',
            dbPort: 3306,
            dbName: '',
            dbUser: '',
            dbPass: '',
            redisHost: '',
            redisPort: 6379,
            jwtSecret: 'test_secret_key_at_least_32_characters_long_xx',
            jwtAlgo: JwtAlgorithm::HS256,
            jwtAccessTtl: 900,
            jwtRefreshTtl: 1209600,
            emailVerifyTtl: 86400,
            appBaseUrl: 'http://localhost:9300/',
        );
        $db = $this->createMock(ConnectionInterface::class);
        $db->method('transaction')->willReturnCallback(static fn (callable $work): mixed => $work());

        $service = new UserService(
            $users,
            $this->createMock(EmailVerificationRepositoryInterface::class),
            $this->createMock(RefreshTokenRepositoryInterface::class),
            $this->createMock(QueueInterface::class),
            $config,
            $clock,
            $db,
            new JwtIssuer(
                Configuration::forSymmetricSigner(new Sha256(), InMemory::plainText($config->jwtSecret)),
                $config,
                $clock,
                new Jwks($config),
            ),
        );

        return new CreateOperator($service);
    }
}
