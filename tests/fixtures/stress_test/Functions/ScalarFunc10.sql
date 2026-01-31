CREATE FUNCTION [dbo].[ScalarFunc10]
(
    @Id INT
)
RETURNS NVARCHAR(100)
AS
BEGIN
    DECLARE @Result NVARCHAR(100);
    
    SELECT @Result = [Name]
    FROM [dbo].[Table11]
    WHERE [Id] = @Id;
    
    RETURN @Result;
END
GO
