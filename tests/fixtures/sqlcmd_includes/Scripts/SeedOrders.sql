-- Seed sample orders
PRINT 'Seeding orders...';
INSERT INTO [dbo].[Orders] ([UserId], [OrderDate])
VALUES
    (1, GETDATE()),
    (1, DATEADD(day, -1, GETDATE())),
    (2, GETDATE());
