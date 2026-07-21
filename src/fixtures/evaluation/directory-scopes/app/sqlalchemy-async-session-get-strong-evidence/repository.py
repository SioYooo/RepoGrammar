from sqlalchemy.ext.asyncio import AsyncSession


class UserRepository:
    async def get_user(self, db: AsyncSession):
        return await db.get(User, 1)

    async def get_account(self, db: AsyncSession):
        return await db.get(Account, 2)

    async def get_team(self, db: AsyncSession):
        return await db.get(Team, 3)


class User:
    pass


class Account:
    pass


class Team:
    pass
