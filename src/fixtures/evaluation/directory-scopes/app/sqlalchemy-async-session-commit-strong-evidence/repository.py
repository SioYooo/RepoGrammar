from sqlalchemy.ext.asyncio import AsyncSession


class UserRepository:
    async def create_user(self, db: AsyncSession):
        await db.commit()

    async def update_user(self, db: AsyncSession):
        await db.commit()

    async def delete_user(self, db: AsyncSession):
        await db.commit()
